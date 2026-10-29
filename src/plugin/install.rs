//! Plugin packages on disk: install / list / uninstall, plus the trusted publisher keys.
//!
//! A managed plugin lives in `<config dir>/plugins/<id>/` with its `plugin.json`, the wasm entry and
//! an `installed.json` record (sha256 at install time + trust verdict). Installing copies a package
//! there atomically; loading a package whose wasm no longer matches the record is refused.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;
use crate::plugin::manifest::{
    decode_public_key, is_valid_id, sha256_hex, verify_signature, Permissions, PluginManifest,
    Trust, INSTALL_FILE, MANIFEST_FILE,
};

/// Largest wasm entry we accept (WASM plugins get loaded into memory, keep it sane).
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// How many files a package may contain.
const MAX_PACKAGE_FILES: usize = 4096;

/// Default managed-plugins root: `<config dir>/plugins`.
pub fn plugins_root() -> PathBuf {
    config::config_dir().join("plugins")
}

/// Default trust store: `<config dir>/plugin_keys.json`.
pub fn trust_store_path() -> PathBuf {
    config::config_dir().join("plugin_keys.json")
}

/// One trusted publisher key (base64 ed25519 public key).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedKey {
    pub name: String,
    pub public_key: String,
}

/// `plugin_keys.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustStore {
    #[serde(default)]
    pub keys: Vec<TrustedKey>,
}

/// Record written into the package on install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRecord {
    pub id: String,
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub signature: Option<String>,
    pub trust: Trust,
    pub installed_at: DateTime<Utc>,
    pub source: String,
}

/// What the REST API / CLI show for one managed plugin.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub homepage: Option<String>,
    pub permissions: Permissions,
    /// Compact permission summary (`fs:r net:api.example.com`).
    pub permission_summary: String,
    /// Wasm entry, relative to `path`.
    pub entry: String,
    pub trust: Trust,
    pub sha256: String,
    pub wasm_bytes: u64,
    pub path: String,
    pub installed_at: Option<DateTime<Utc>>,
    /// `false` when `config.plugins.<id>.enabled` is false.
    pub enabled: bool,
    /// `false` for untrusted / tampered packages (kept on disk, never loaded).
    pub loadable: bool,
}

// ---------------------------------------------------------------------------
// Trust store
// ---------------------------------------------------------------------------

/// Read the trust store; a missing file is an empty store, a broken one is logged and ignored.
pub fn load_trust_store(path: &Path) -> TrustStore {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<TrustStore>(&text) {
            Ok(store) => store,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "plugin trust store is not valid JSON");
                TrustStore::default()
            }
        },
        Err(_) => TrustStore::default(),
    }
}

pub fn save_trust_store(path: &Path, store: &TrustStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(store)?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Add (or replace, by name) a trusted publisher key.
pub fn add_trusted_key(path: &Path, name: &str, public_key_b64: &str) -> Result<TrustedKey> {
    let name = name.trim();
    if name.is_empty() {
        bail!("key name must not be empty");
    }
    let key = decode_public_key(public_key_b64)?;
    let mut store = load_trust_store(path);
    let entry = TrustedKey {
        name: name.to_string(),
        public_key: base64_encode(&key),
    };
    match store.keys.iter_mut().find(|k| k.name == name) {
        Some(existing) => *existing = entry.clone(),
        None => store.keys.push(entry.clone()),
    }
    save_trust_store(path, &store)?;
    Ok(entry)
}

/// Remove a trusted key by name. Returns whether something was removed.
pub fn remove_trusted_key(path: &Path, name: &str) -> Result<bool> {
    let mut store = load_trust_store(path);
    let before = store.keys.len();
    store.keys.retain(|k| k.name != name);
    let removed = store.keys.len() != before;
    if removed {
        save_trust_store(path, &store)?;
    }
    Ok(removed)
}

/// Decoded public keys of the trust store (invalid entries are skipped with a warning).
pub fn trusted_key_bytes(path: &Path) -> Vec<Vec<u8>> {
    load_trust_store(path)
        .keys
        .iter()
        .filter_map(|k| match decode_public_key(&k.public_key) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                tracing::warn!(name = %k.name, error = %e, "skipping invalid plugin public key");
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Install / list / uninstall
// ---------------------------------------------------------------------------

/// Install a package directory into `root`. Re-installing the same id replaces it atomically.
pub fn install_from_dir(root: &Path, source: &Path, trust_path: &Path) -> Result<InstalledPlugin> {
    let source = source
        .canonicalize()
        .with_context(|| format!("plugin package {} is not accessible", source.display()))?;
    if !source.is_dir() {
        bail!("{} is not a directory", source.display());
    }
    let manifest_path = source.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("missing {}", manifest_path.display()))?;
    let manifest = PluginManifest::parse(&text)?;

    let entry = source.join(&manifest.entry);
    let entry = entry
        .canonicalize()
        .with_context(|| format!("entry {} is missing", manifest.entry))?;
    if !entry.starts_with(&source) {
        bail!("entry {} escapes the package root", manifest.entry);
    }
    let meta = std::fs::metadata(&entry)?;
    if !meta.is_file() {
        bail!("entry {} is not a file", manifest.entry);
    }
    if meta.len() > MAX_ENTRY_BYTES {
        bail!(
            "entry {} is {} bytes (limit {})",
            manifest.entry,
            meta.len(),
            MAX_ENTRY_BYTES
        );
    }
    let bytes = std::fs::read(&entry)?;
    let digest = sha256_hex(&bytes);

    if let Some(expected) = &manifest.sha256 {
        if !expected.eq_ignore_ascii_case(&digest) {
            bail!(
                "plugin '{}' sha256 mismatch: manifest says {}, file is {}",
                manifest.id,
                expected,
                digest
            );
        }
    }

    let trust = match &manifest.signature {
        Some(signature) => {
            let keys = trusted_key_bytes(trust_path);
            verify_signature(&manifest.signing_payload(&digest), signature, &keys)?
        }
        None => Trust::Unsigned,
    };

    let id = manifest.id.clone();
    let staging = root.join(format!(".staging-{}-{}", id, unique_suffix()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).ok();
    }
    std::fs::create_dir_all(&staging).with_context(|| format!("create {}", staging.display()))?;
    copy_package(&source, &staging)?;

    let record = InstallRecord {
        id: id.clone(),
        version: manifest.version.clone(),
        sha256: digest.clone(),
        signature: manifest.signature.clone(),
        trust,
        installed_at: Utc::now(),
        source: source.display().to_string(),
    };
    let record_text = serde_json::to_string_pretty(&record)?;
    std::fs::write(staging.join(INSTALL_FILE), record_text)?;
    // Pin the digest into the installed manifest so a later edit is detected twice over.
    let pinned = manifest.clone().with_digest(&digest);
    std::fs::write(
        staging.join(MANIFEST_FILE),
        serde_json::to_string_pretty(&pinned)?,
    )?;

    let target = root.join(&id);
    let backup = root.join(format!(".old-{}-{}", id, unique_suffix()));
    if target.exists() {
        std::fs::rename(&target, &backup)
            .with_context(|| format!("move {} aside", target.display()))?;
    }
    std::fs::rename(&staging, &target)
        .with_context(|| format!("install into {}", target.display()))?;
    if backup.exists() {
        std::fs::remove_dir_all(&backup).ok();
    }

    Ok(InstalledPlugin {
        id: id.clone(),
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        author: manifest.author,
        homepage: manifest.homepage,
        permission_summary: manifest.permissions.summary(),
        permissions: manifest.permissions,
        entry: manifest.entry,
        trust,
        sha256: digest,
        wasm_bytes: meta.len(),
        path: target.display().to_string(),
        installed_at: Some(record.installed_at),
        enabled: true,
        loadable: trust.is_loadable(),
    })
}

/// Every managed package under `root`, oldest id first. `configs` decides `enabled`.
pub fn list(root: &Path, configs: &HashMap<String, Value>) -> Vec<InstalledPlugin> {
    let mut out: Vec<InstalledPlugin> = Vec::new();
    let Ok(dir) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !path.is_dir() {
            continue;
        }
        match read_installed(&path, configs) {
            Ok(plugin) => out.push(plugin),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skip unreadable plugin package");
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Read one installed package (manifest + install record + current digest).
pub fn read_installed(dir: &Path, configs: &HashMap<String, Value>) -> Result<InstalledPlugin> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .with_context(|| format!("missing {}", MANIFEST_FILE))?;
    let manifest = PluginManifest::parse(&text)?;
    let record: Option<InstallRecord> = std::fs::read_to_string(dir.join(INSTALL_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());

    let entry = dir.join(&manifest.entry);
    let wasm_bytes = std::fs::metadata(&entry).map(|m| m.len()).unwrap_or(0);
    let digest = std::fs::read(&entry)
        .map(|b| sha256_hex(&b))
        .unwrap_or_default();

    let trust = match &record {
        Some(record) if !record.sha256.is_empty() && record.sha256 != digest => Trust::Tampered,
        Some(record) => match &manifest.signature {
            Some(signature) => {
                let keys = trusted_key_bytes(&trust_store_path());
                if keys.is_empty() {
                    record.trust
                } else {
                    verify_signature(&manifest.signing_payload(&digest), signature, &keys)
                        .unwrap_or(Trust::Untrusted)
                }
            }
            None => record.trust,
        },
        None => match &manifest.signature {
            Some(_) => Trust::Untrusted,
            None => Trust::Unsigned,
        },
    };

    let enabled = configs
        .get(&manifest.id)
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(InstalledPlugin {
        id: manifest.id.clone(),
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        author: manifest.author,
        homepage: manifest.homepage,
        permission_summary: manifest.permissions.summary(),
        permissions: manifest.permissions,
        entry: manifest.entry,
        trust,
        sha256: digest,
        wasm_bytes,
        path: dir.display().to_string(),
        installed_at: record.as_ref().map(|r| r.installed_at),
        enabled,
        loadable: trust.is_loadable(),
    })
}

/// Remove a managed package. Only directories directly under `root` can be removed.
pub fn uninstall(root: &Path, id: &str) -> Result<bool> {
    if !is_valid_id(id) {
        bail!("invalid plugin id '{id}'");
    }
    let root_canon = root
        .canonicalize()
        .with_context(|| format!("plugins dir {} is not accessible", root.display()))?;
    let target = root.join(id);
    if !target.exists() {
        return Ok(false);
    }
    let target_canon = target.canonicalize()?;
    if target_canon.parent() != Some(root_canon.as_path()) {
        bail!("refusing to remove {}", target_canon.display());
    }
    std::fs::remove_dir_all(&target_canon)
        .with_context(|| format!("remove {}", target_canon.display()))?;
    Ok(true)
}

fn copy_package(source: &Path, target: &Path) -> Result<()> {
    let mut files = 0usize;
    copy_dir_inner(source, target, &mut files)?;
    if files > MAX_PACKAGE_FILES {
        bail!("package has more than {MAX_PACKAGE_FILES} files");
    }
    Ok(())
}

fn copy_dir_inner(source: &Path, target: &Path, files: &mut usize) -> Result<()> {
    for entry in std::fs::read_dir(source)?.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let dest = target.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_dir_inner(&path, &dest, files)?;
        } else if file_type.is_file() {
            *files += 1;
            if *files > MAX_PACKAGE_FILES {
                bail!("package has more than {MAX_PACKAGE_FILES} files");
            }
            std::fs::copy(&path, &dest).with_context(|| format!("copy {}", path.display()))?;
        }
        // Symlinks are skipped: a package must be self-contained.
    }
    Ok(())
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    BASE64.encode(bytes)
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::PluginManifest;
    use ed25519_dalek::{Signer, SigningKey};

    fn write_package(dir: &Path, manifest: &PluginManifest, wasm: &[u8]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_string(manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(&manifest.entry), wasm).unwrap();
    }

    fn demo_manifest(perms: Permissions) -> PluginManifest {
        PluginManifest {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            version: "1.0.0".to_string(),
            description: "demo plugin".to_string(),
            author: "tester".to_string(),
            homepage: None,
            entry: "demo.wasm".to_string(),
            permissions: perms,
            sha256: None,
            signature: None,
        }
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("plugins");
        let trust = tmp.path().join("plugin_keys.json");
        std::fs::create_dir_all(&root).unwrap();
        (tmp, root, trust)
    }

    #[test]
    fn install_records_digest_and_lists_the_package() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        let wasm = b"\0asm\x01\0\0\0";
        write_package(&src, &demo_manifest(Permissions::default()), wasm);

        let installed = install_from_dir(&root, &src, &trust).expect("install");
        assert_eq!(installed.id, "demo");
        assert_eq!(installed.trust, Trust::Unsigned);
        assert_eq!(installed.sha256, sha256_hex(wasm));
        assert_eq!(installed.permission_summary, "none");
        assert!(installed.loadable);

        let listed = list(&root, &HashMap::new());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "demo");
        assert!(listed[0].enabled);
        assert_eq!(listed[0].trust, Trust::Unsigned);
    }

    #[test]
    fn install_is_idempotent_and_replaces_previous_version() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        write_package(
            &src,
            &demo_manifest(Permissions::default()),
            b"\0asm\x01\0\0\0",
        );
        install_from_dir(&root, &src, &trust).unwrap();

        let mut manifest = demo_manifest(Permissions::default());
        manifest.version = "1.1.0".to_string();
        write_package(&src, &manifest, b"\0asm\x02\0\0\0");
        install_from_dir(&root, &src, &trust).unwrap();

        let listed = list(&root, &HashMap::new());
        assert_eq!(listed.len(), 1, "the package is replaced, not duplicated");
        assert_eq!(listed[0].version, "1.1.0");
        // No staging/backup leftovers.
        let entries: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["demo".to_string()]);
    }

    #[test]
    fn tampering_with_the_wasm_is_detected() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        write_package(
            &src,
            &demo_manifest(Permissions::default()),
            b"\0asm\x01\0\0\0",
        );
        install_from_dir(&root, &src, &trust).unwrap();

        std::fs::write(root.join("demo/demo.wasm"), b"\0asm\xff\0\0\0").unwrap();
        let listed = list(&root, &HashMap::new());
        assert_eq!(listed[0].trust, Trust::Tampered);
        assert!(!listed[0].loadable);
    }

    #[test]
    fn manifest_digest_mismatch_is_rejected() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        let mut manifest = demo_manifest(Permissions::default());
        manifest.sha256 = Some("0".repeat(64));
        write_package(&src, &manifest, b"\0asm\x01\0\0\0");

        let err = install_from_dir(&root, &src, &trust).expect_err("digest mismatch");
        assert!(err.to_string().contains("sha256 mismatch"), "{err}");
    }

    #[test]
    fn signed_package_is_trusted_only_with_a_known_key() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        let wasm = b"\0asm\x01\0\0\0";
        let digest = sha256_hex(wasm);

        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let payload = format!("demo@1.0.0:{digest}");
        let signature = base64_encode(&signing.sign(payload.as_bytes()).to_bytes());
        let mut manifest = demo_manifest(Permissions::default());
        manifest.signature = Some(signature);
        write_package(&src, &manifest, wasm);

        // Unknown key → installed but inert.
        let installed = install_from_dir(&root, &src, &trust).unwrap();
        assert_eq!(installed.trust, Trust::Untrusted);
        assert!(!installed.loadable);

        // Trust the publisher → trusted and loadable.
        let public = base64_encode(signing.verifying_key().as_bytes());
        add_trusted_key(&trust, "tester", &public).unwrap();
        let installed = install_from_dir(&root, &src, &trust).unwrap();
        assert_eq!(installed.trust, Trust::Trusted);
        assert!(installed.loadable);
        assert_eq!(list(&root, &HashMap::new())[0].trust, Trust::Trusted);
    }

    #[test]
    fn uninstall_only_touches_direct_children() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        write_package(
            &src,
            &demo_manifest(Permissions::default()),
            b"\0asm\x01\0\0\0",
        );
        install_from_dir(&root, &src, &trust).unwrap();

        assert!(!uninstall(&root, "missing").unwrap());
        assert!(uninstall(&root, "demo").unwrap());
        assert!(list(&root, &HashMap::new()).is_empty());
        assert!(uninstall(&root, "../plugins/demo").is_err());
    }

    #[test]
    fn enabled_flag_comes_from_config() {
        let (tmp, root, trust) = setup();
        let src = tmp.path().join("pkg");
        write_package(
            &src,
            &demo_manifest(Permissions::default()),
            b"\0asm\x01\0\0\0",
        );
        install_from_dir(&root, &src, &trust).unwrap();

        let mut configs = HashMap::new();
        configs.insert("demo".to_string(), serde_json::json!({ "enabled": false }));
        assert!(!list(&root, &configs)[0].enabled);
        configs.insert("demo".to_string(), serde_json::json!({ "enabled": true }));
        assert!(list(&root, &configs)[0].enabled);
    }

    #[test]
    fn trust_store_roundtrip_and_rejects_bad_keys() {
        let (tmp, _root, trust) = setup();
        let signing = SigningKey::from_bytes(&[3u8; 32]);
        let public = base64_encode(signing.verifying_key().as_bytes());

        add_trusted_key(&trust, "alice", &public).unwrap();
        add_trusted_key(&trust, "alice", &public).unwrap(); // same name replaces
        assert_eq!(load_trust_store(&trust).keys.len(), 1);
        assert_eq!(trusted_key_bytes(&trust).len(), 1);

        assert!(add_trusted_key(&trust, "bob", "not-base64").is_err());
        assert!(add_trusted_key(&trust, "bob", &base64_encode(&[0u8; 16])).is_err());
        assert!(add_trusted_key(&trust, " ", &public).is_err());

        assert!(remove_trusted_key(&trust, "alice").unwrap());
        assert!(!remove_trusted_key(&trust, "alice").unwrap());
        assert!(trusted_key_bytes(&trust).is_empty());
        assert!(tmp.path().exists());
    }
}
