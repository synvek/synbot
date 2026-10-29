//! Plugin package manifest: identity, permissions and integrity.
//!
//! A packaged plugin is a directory that contains `plugin.json` and the wasm entry it names:
//!
//! ```json
//! {
//!   "id": "hello-tool",
//!   "name": "Hello Tool",
//!   "version": "1.0.0",
//!   "entry": "hello_tool.wasm",
//!   "permissions": {
//!     "filesystem": { "read": true, "write": false, "paths": ["docs"] },
//!     "network": { "hosts": ["api.example.com"] }
//!   },
//!   "sha256": "<hex>",
//!   "signature": "<base64 ed25519 over \"{id}@{version}:{sha256}\">"
//! }
//! ```
//!
//! Everything the plugin can reach is decided here: the host functions check
//! [`HostPermissions`] on every call, and Extism gets the same budget (timeout / memory) no matter
//! what the wasm asks for. Legacy plugins that only have a `path` in the config keep the old
//! behaviour through [`HostPermissions::unrestricted`].

use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

/// File name of the manifest inside a plugin package.
pub const MANIFEST_FILE: &str = "plugin.json";
/// Install record written next to the manifest (`sha256` at install time + trust verdict).
pub const INSTALL_FILE: &str = "installed.json";
/// Longest accepted plugin id.
const MAX_ID_LEN: usize = 64;

fn default_true() -> bool {
    true
}

/// Filesystem permissions granted to a plugin.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilesystemPermission {
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
    /// Workspace-relative prefixes. Empty means the whole workspace (still never outside it).
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Network permissions granted to a plugin.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkPermission {
    /// `api.example.com`, `*.example.com` or `*`. Empty means no network at all.
    #[serde(default)]
    pub hosts: Vec<String>,
}

/// Everything a plugin is allowed to touch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permissions {
    #[serde(default)]
    pub filesystem: FilesystemPermission,
    #[serde(default)]
    pub network: NetworkPermission,
    /// Read this plugin's own `config.plugins.<id>` entry through `config_get`.
    #[serde(default = "default_true")]
    pub config: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            filesystem: FilesystemPermission::default(),
            network: NetworkPermission::default(),
            config: true,
        }
    }
}

impl Permissions {
    /// Compact human summary used by the CLI/REST/UI (`fs:r`, `net:*.example.com`, …).
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let fs = &self.filesystem;
        match (fs.read, fs.write) {
            (true, true) => parts.push("fs:rw".to_string()),
            (true, false) => parts.push("fs:r".to_string()),
            (false, true) => parts.push("fs:w".to_string()),
            (false, false) => {}
        }
        if !fs.paths.is_empty() {
            parts.push(format!("fs:path={}", fs.paths.join(",")));
        }
        if !self.network.hosts.is_empty() {
            parts.push(format!("net:{}", self.network.hosts.join(",")));
        }
        if !self.config {
            parts.push("config:deny".to_string());
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(" ")
        }
    }
}

/// `plugin.json` — the trusted description of a plugin package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: Option<String>,
    /// Wasm entry, relative to the package root (`..`, absolute paths and nested escapes rejected).
    pub entry: String,
    #[serde(default)]
    pub permissions: Permissions,
    /// Lowercase hex sha256 of the entry file. Computed and recorded on install when omitted.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Base64 ed25519 signature over [`signing_payload`].
    #[serde(default)]
    pub signature: Option<String>,
}

impl PluginManifest {
    /// Parse a manifest from JSON text.
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(text).context("plugin.json is not valid JSON")?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Structural checks that do not need the wasm file to exist.
    pub fn validate(&self) -> Result<()> {
        if !is_valid_id(&self.id) {
            bail!(
                "invalid plugin id '{}': use 1-{} lowercase letters, digits, '-' or '_' (start with a letter/digit)",
                self.id,
                MAX_ID_LEN
            );
        }
        if self.name.trim().is_empty() {
            bail!("plugin '{}' has an empty name", self.id);
        }
        if self.version.trim().is_empty() {
            bail!("plugin '{}' has an empty version", self.id);
        }
        validate_entry(&self.entry)?;
        for path in &self.permissions.filesystem.paths {
            validate_relative_path(path)
                .with_context(|| format!("plugin '{}' filesystem path", self.id))?;
        }
        for host in &self.permissions.network.hosts {
            if !is_valid_host_pattern(host) {
                bail!(
                    "plugin '{}' has an invalid host pattern '{}'",
                    self.id,
                    host
                );
            }
        }
        if let Some(sha) = &self.sha256 {
            if sha.len() != 64 || hex::decode(sha).is_err() {
                bail!("plugin '{}' sha256 must be 64 hex characters", self.id);
            }
        }
        Ok(())
    }

    /// Text that the ed25519 signature covers.
    pub fn signing_payload(&self, sha256_hex: &str) -> String {
        format!("{}@{}:{}", self.id, self.version, sha256_hex)
    }

    /// Manifest with the digest filled in (used when the author did not pin one).
    pub fn with_digest(mut self, sha256_hex: &str) -> Self {
        self.sha256 = Some(sha256_hex.to_string());
        self
    }
}

/// Trust verdict for an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    /// Signature verified against a trusted public key.
    Trusted,
    /// No signature at all (locally built / hand-dropped package).
    Unsigned,
    /// Signature present but no trusted key matched it.
    Untrusted,
    /// The wasm no longer matches the digest recorded at install time.
    Tampered,
}

impl Trust {
    /// Only trusted/unsigned packages may be loaded; untrusted or tampered ones stay on disk but inert.
    pub fn is_loadable(self) -> bool {
        matches!(self, Trust::Trusted | Trust::Unsigned)
    }
}

/// Runtime view of a plugin's permissions: what the host functions are allowed to do.
#[derive(Debug, Clone, Default)]
pub struct HostPermissions {
    pub read: bool,
    pub write: bool,
    /// Workspace-relative prefixes (empty = whole workspace).
    pub paths: Vec<String>,
    /// Allowed hosts (`*`, `*.example.com`, `api.example.com`).
    pub hosts: Vec<String>,
    pub config: bool,
}

impl HostPermissions {
    pub fn from_permissions(permissions: &Permissions) -> Self {
        Self {
            read: permissions.filesystem.read,
            write: permissions.filesystem.write,
            paths: permissions.filesystem.paths.clone(),
            hosts: permissions.network.hosts.clone(),
            config: permissions.config,
        }
    }

    /// Legacy plugins declared only by config keep the pre-manifest behaviour.
    pub fn unrestricted() -> Self {
        Self {
            read: true,
            write: true,
            paths: Vec::new(),
            hosts: vec!["*".to_string()],
            config: true,
        }
    }

    /// `http_request` gate: parse the URL and match its host against the allowlist.
    pub fn allows_http(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = parsed.host_str() else {
            return false;
        };
        self.allows_host(host)
    }

    /// Host allowlist match (`*`, `*.example.com`, exact).
    pub fn allows_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.iter().any(|pattern| {
            let pattern = pattern.trim_end_matches('.').to_ascii_lowercase();
            match pattern.strip_prefix("*.") {
                Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
                None => pattern == "*" || pattern == host,
            }
        })
    }

    /// `fs_*` gate: the path must be relative, stay inside the workspace and match `paths`.
    pub fn allows_relative_path(&self, relative: &str) -> bool {
        // Normalise first: `..` escapes must be rejected even when `paths` is empty, otherwise an
        // empty allowlist would read as "the whole filesystem" instead of "the whole workspace".
        let normalized = match normalize_relative(relative) {
            Some(p) => p,
            None => return false,
        };
        if self.paths.is_empty() {
            return true;
        }
        self.paths.iter().any(|prefix| {
            let prefix = prefix.trim_matches('/');
            prefix.is_empty()
                || normalized == prefix
                || normalized.starts_with(&format!("{prefix}/"))
        })
    }
}

/// Lowercase hex sha256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Verify `signature_b64` over `payload` against the trusted keys (raw 32-byte public keys).
pub fn verify_signature(
    payload: &str,
    signature_b64: &str,
    trusted_keys: &[Vec<u8>],
) -> Result<Trust> {
    let raw = BASE64
        .decode(signature_b64.trim())
        .context("signature is not valid base64")?;
    let signature = Signature::from_slice(&raw).context("signature must be 64 bytes")?;
    for key in trusted_keys {
        let Ok(bytes): Result<[u8; 32], _> = key.as_slice().try_into() else {
            continue;
        };
        let Ok(verifying) = VerifyingKey::from_bytes(&bytes) else {
            continue;
        };
        if verifying
            .verify_strict(payload.as_bytes(), &signature)
            .is_ok()
        {
            return Ok(Trust::Trusted);
        }
    }
    Ok(Trust::Untrusted)
}

/// Public key bytes from a base64 string (used by the trust store).
pub fn decode_public_key(public_key_b64: &str) -> Result<Vec<u8>> {
    let raw = BASE64
        .decode(public_key_b64.trim())
        .context("public key is not valid base64")?;
    if raw.len() != 32 {
        bail!("public key must be 32 bytes (got {})", raw.len());
    }
    // Reject keys that are not a valid ed25519 point.
    let bytes: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&bytes).context("public key is not a valid ed25519 key")?;
    Ok(raw)
}

/// `^[a-z0-9][a-z0-9_-]{0,63}$` (rendered ids are also used as directory names).
pub fn is_valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return false;
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn validate_entry(entry: &str) -> Result<()> {
    if entry.trim().is_empty() {
        bail!("entry must name the wasm file");
    }
    let path = Path::new(entry);
    if path.is_absolute() {
        bail!("entry must be relative to the package root");
    }
    if !entry.ends_with(".wasm") {
        bail!("entry must be a .wasm file");
    }
    if normalize_relative(entry).is_none() {
        bail!("entry must not escape the package root");
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<()> {
    if path.trim().is_empty() {
        bail!("filesystem paths must not be empty (omit the list to allow the whole workspace)");
    }
    if Path::new(path).is_absolute() || normalize_relative(path).is_none() {
        bail!("'{}' must be a relative path inside the workspace", path);
    }
    Ok(())
}

fn is_valid_host_pattern(host: &str) -> bool {
    let host = host.trim();
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if host == "*" {
        return true;
    }
    let bare = host.strip_prefix("*.").unwrap_or(host);
    bare.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// Normalise a workspace-relative path, returning `None` for absolute paths or `..` escapes.
fn normalize_relative(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => return None,
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn manifest_with(permissions: Permissions) -> PluginManifest {
        PluginManifest {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            homepage: None,
            entry: "demo.wasm".to_string(),
            permissions,
            sha256: None,
            signature: None,
        }
    }

    #[test]
    fn ids_must_be_directory_safe() {
        assert!(is_valid_id("demo"));
        assert!(is_valid_id("demo-tool_2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("../escape"));
        assert!(!is_valid_id("Upper"));
        assert!(!is_valid_id("with space"));
        assert!(!is_valid_id(&"a".repeat(MAX_ID_LEN + 1)));
    }

    #[test]
    fn manifest_rejects_escaping_entry_and_paths() {
        let mut m = manifest_with(Permissions::default());
        assert!(m.validate().is_ok());
        m.entry = "../evil.wasm".to_string();
        assert!(m.validate().is_err());
        m.entry = "/abs/evil.wasm".to_string();
        assert!(m.validate().is_err());
        m.entry = "demo.wat".to_string();
        assert!(m.validate().is_err());

        let mut m = manifest_with(Permissions {
            filesystem: FilesystemPermission {
                read: true,
                write: false,
                paths: vec!["../../etc".to_string()],
            },
            ..Permissions::default()
        });
        assert!(m.validate().is_err());
        m.permissions.filesystem.paths = vec!["docs".to_string()];
        assert!(m.validate().is_ok());
    }

    #[test]
    fn permissions_are_deny_by_default() {
        let permissions = Permissions::default();
        assert!(!permissions.filesystem.read);
        assert!(!permissions.filesystem.write);
        assert!(permissions.network.hosts.is_empty());
        assert!(permissions.config);
        assert_eq!(permissions.summary(), "none");

        let host = HostPermissions::from_permissions(&permissions);
        assert!(!host.allows_http("https://example.com"));
        // The read/write flags are checked before this helper, so an empty `paths` list only means
        // "anywhere inside the workspace" once access has been granted at all.
        assert!(!host.read && !host.write);
        assert!(host.allows_relative_path("anything"));
        assert!(!host.allows_relative_path("../outside.txt"));
    }

    #[test]
    fn host_allowlist_supports_exact_wildcard_and_any() {
        let host = HostPermissions {
            read: false,
            write: false,
            paths: Vec::new(),
            hosts: vec!["api.example.com".to_string(), "*.example.org".to_string()],
            config: true,
        };
        assert!(host.allows_http("https://api.example.com/v1?q=1"));
        assert!(host.allows_host("API.EXAMPLE.COM"));
        assert!(!host.allows_http("https://evil.example.com"));
        assert!(host.allows_http("https://example.org"));
        assert!(host.allows_http("https://deep.example.org"));
        assert!(!host.allows_http("https://example.org.evil.com"));
        assert!(!host.allows_http("ftp://api.example.com"));
        assert!(!host.allows_http("not a url"));

        let any = HostPermissions {
            hosts: vec!["*".to_string()],
            ..HostPermissions::default()
        };
        assert!(any.allows_http("http://anything.test/path"));
    }

    #[test]
    fn path_prefixes_gate_reads_and_writes() {
        let host = HostPermissions {
            read: true,
            write: false,
            paths: vec!["docs".to_string(), "src/lib".to_string()],
            hosts: Vec::new(),
            config: true,
        };
        assert!(host.allows_relative_path("docs/readme.md"));
        assert!(host.allows_relative_path("./docs/readme.md"));
        assert!(host.allows_relative_path("src/lib/mod.rs"));
        assert!(!host.allows_relative_path("src/main.rs"));
        assert!(!host.allows_relative_path("docs/../secret.txt"));
        assert!(!host.allows_relative_path("/etc/passwd"));

        let whole = HostPermissions {
            paths: Vec::new(),
            ..host
        };
        assert!(whole.allows_relative_path("anything/at/all.txt"));
        assert!(!whole.allows_relative_path("../outside.txt"));
    }

    #[test]
    fn signature_verification_distinguishes_trusted_unsigned_and_untrusted() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public = signing.verifying_key().to_bytes().to_vec();
        let payload = format!("demo@1.0.0:{}", "a".repeat(64));
        let signature = BASE64.encode(signing.sign(payload.as_bytes()).to_bytes());

        assert_eq!(
            verify_signature(&payload, &signature, std::slice::from_ref(&public)).unwrap(),
            Trust::Trusted
        );
        assert_eq!(
            verify_signature("other@1.0.0:deadbeef", &signature, &[public]).unwrap(),
            Trust::Untrusted
        );
        assert_eq!(
            verify_signature(&payload, &signature, &[]).unwrap(),
            Trust::Untrusted
        );
        assert!(verify_signature(&payload, "not-base64!", &[]).is_err());
    }

    #[test]
    fn sha256_is_hex_and_stable() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b"").len(), 64);
    }

    #[test]
    fn manifest_parses_minimal_json_with_defaults() {
        let parsed = PluginManifest::parse(
            r#"{"id":"demo","name":"Demo","version":"0.1.0","entry":"demo.wasm"}"#,
        )
        .expect("valid manifest");
        assert_eq!(parsed.permissions, Permissions::default());
        assert!(parsed.permissions.config);
        assert!(!parsed.permissions.filesystem.read);
        assert!(parsed.with_digest(&sha256_hex(b"x")).sha256.is_some());
    }

    #[test]
    fn trust_loadability() {
        assert!(Trust::Trusted.is_loadable());
        assert!(Trust::Unsigned.is_loadable());
        assert!(!Trust::Untrusted.is_loadable());
        assert!(!Trust::Tampered.is_loadable());
    }
}
