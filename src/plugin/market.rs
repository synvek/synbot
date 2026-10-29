//! Plugin market: index sources, dependency resolution and download-then-install.
//!
//! A market is a JSON index (listed in `pluginMarket.sources`) that describes published versions:
//!
//! ```json
//! {
//!   "name": "Acme plugins",
//!   "plugins": [
//!     {
//!       "id": "hello-tool",
//!       "name": "Hello Tool",
//!       "version": "1.2.0",
//!       "description": "Greets people",
//!       "author": "acme",
//!       "manifest_url": "https://plugins.example.com/hello-tool/1.2.0/plugin.json",
//!       "entry_url": "https://plugins.example.com/hello-tool/1.2.0/hello_tool.wasm",
//!       "sha256": "<hex of the wasm>",
//!       "dependencies": [{ "id": "base-tools", "version": ">=1.0.0" }]
//!     }
//!   ]
//! }
//! ```
//!
//! Installs are staged into `<plugins>/.market-<id>-<suffix>/`, verified file by file against the
//! digests in the index, then handed to [`crate::plugin::install`] — the same atomic replace and
//! trust verdict a hand-installed package goes through. A market therefore cannot grant a plugin
//! more than a local install would: permissions still come from `plugin.json`, and signatures are
//! still checked against the trust store.
//!
//! A source may be `https://…`, `file://…` or a plain path (resolved against the config dir), so a
//! team can publish a directory of packages on a shared disk without running a server.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::appcontainer_dns::build_reqwest_client_with_timeout;
use crate::config;
use crate::plugin::install::{self, InstalledPlugin};
use crate::plugin::manifest::{
    is_valid_id, sha256_hex, validate_relative_path, PluginManifest, Trust, INSTALL_FILE,
    MANIFEST_FILE,
};

/// Largest index file we read (a full index is kilobytes; this only stops a hostile source).
const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;
/// Largest single downloaded file (manifests are tiny, wasm entries are capped by the installer).
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Largest `plugin.json` we accept.
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

// ---------------------------------------------------------------------------
// Versions and requirements
// ---------------------------------------------------------------------------

/// A `major.minor.patch` version with an optional pre-release tag (`1.2.0-beta.1`).
///
/// Build metadata (`+build`) is ignored, pre-releases sort *below* the matching release, and a
/// missing component counts as zero (`1.2` == `1.2.0`) — enough to order a plugin index without
/// pulling in a semver crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<String>,
}

impl Version {
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let raw = raw.strip_prefix('v').unwrap_or(raw);
        let (core, rest) = match raw.split_once('+') {
            Some((core, _build)) => (core, None),
            None => match raw.split_once('-') {
                Some((core, pre)) => (core, Some(pre)),
                None => (raw, None),
            },
        };
        let mut parts = core.split('.');
        let mut next = |what: &str| -> Result<u64> {
            match parts.next() {
                Some(part) => part
                    .parse::<u64>()
                    .with_context(|| format!("invalid {what} component in version '{raw}'")),
                None => Ok(0),
            }
        };
        let major = next("major")?;
        let minor = next("minor")?;
        let patch = next("patch")?;
        if parts.next().is_some() {
            bail!("version '{raw}' has more than three components");
        }
        let pre = rest.map(|p| p.to_string()).filter(|p| !p.is_empty());
        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                // A release is newer than its own pre-releases.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Any,
    Exact,
    Caret,
    Tilde,
    Gte,
    Gt,
    Lte,
    Lt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Constraint {
    op: Op,
    version: Option<Version>,
}

/// A version requirement: `>=1.2.0`, `^1.2`, `~1.2.3`, an exact `1.2.3`, or `*`.
///
/// Several comma/space separated terms are ANDed (`>=1.0.0 <2.0.0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    raw: String,
    constraints: Vec<Constraint>,
}

impl Requirement {
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.is_empty() || raw == "*" {
            return Ok(Self {
                raw: "*".to_string(),
                constraints: vec![Constraint {
                    op: Op::Any,
                    version: None,
                }],
            });
        }
        let mut constraints = Vec::new();
        for token in raw.split([',', ' ']).filter(|t| !t.is_empty()) {
            if token == "*" {
                continue;
            }
            let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
                (Op::Gte, rest)
            } else if let Some(rest) = token.strip_prefix("<=") {
                (Op::Lte, rest)
            } else if let Some(rest) = token.strip_prefix('>') {
                (Op::Gt, rest)
            } else if let Some(rest) = token.strip_prefix('<') {
                (Op::Lt, rest)
            } else if let Some(rest) = token.strip_prefix('=') {
                (Op::Exact, rest)
            } else if let Some(rest) = token.strip_prefix('^') {
                (Op::Caret, rest)
            } else if let Some(rest) = token.strip_prefix('~') {
                (Op::Tilde, rest)
            } else {
                (Op::Exact, token)
            };
            let version = Version::parse(rest)
                .with_context(|| format!("invalid version in requirement '{raw}'"))?;
            constraints.push(Constraint {
                op,
                version: Some(version),
            });
        }
        if constraints.is_empty() {
            bail!("version requirement '{raw}' has no version");
        }
        Ok(Self {
            raw: raw.to_string(),
            constraints,
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.constraints.iter().all(|c| {
            let Some(req) = &c.version else {
                return true;
            };
            match c.op {
                Op::Any => true,
                Op::Exact => version == req,
                // `^1.2.3` = `>=1.2.3 <2.0.0`; `^0.2.3` keeps the minor because 0.x is unstable.
                Op::Caret => {
                    version >= req
                        && version.major == req.major
                        && (req.major > 0 || version.minor == req.minor)
                }
                // `~1.2.3` = `>=1.2.3 <1.3.0`.
                Op::Tilde => {
                    version >= req && version.major == req.major && version.minor == req.minor
                }
                Op::Gte => version >= req,
                Op::Gt => version > req,
                Op::Lte => version <= req,
                Op::Lt => version < req,
            }
        })
    }
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

// ---------------------------------------------------------------------------
// Index format
// ---------------------------------------------------------------------------

/// One extra file shipped next to the wasm entry (assets, data files, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketFile {
    /// Path inside the package (relative, `..` rejected).
    pub path: String,
    pub url: String,
    pub sha256: String,
}

/// A published dependency of another plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketDependency {
    pub id: String,
    /// Version requirement; omitted means "any version".
    #[serde(default)]
    pub version: Option<String>,
}

/// One published plugin version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketEntry {
    pub id: String,
    /// Display name; defaults to the id.
    #[serde(default)]
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Location of the package's `plugin.json`.
    pub manifest_url: String,
    /// Location of the wasm entry; defaults to the manifest's sibling named by `entry`.
    #[serde(default)]
    pub entry_url: Option<String>,
    /// Hex sha256 of the wasm entry — the integrity anchor for the whole entry.
    pub sha256: String,
    #[serde(default)]
    pub files: Vec<MarketFile>,
    #[serde(default)]
    pub dependencies: Vec<MarketDependency>,
}

impl MarketEntry {
    /// Structural checks + defaults, run on every entry that leaves an index.
    fn prepare(mut self) -> Result<Self> {
        self.id = self.id.trim().to_string();
        if !is_valid_id(&self.id) {
            bail!("invalid plugin id '{}'", self.id);
        }
        self.version = self.version.trim().to_string();
        Version::parse(&self.version).with_context(|| format!("plugin '{}' version", self.id))?;
        if self.name.trim().is_empty() {
            self.name = self.id.clone();
        }
        self.author = self.author.trim().to_string();
        self.description = self.description.trim().to_string();
        if self.sha256.len() != 64 || hex::decode(&self.sha256).is_err() {
            bail!("plugin '{}' sha256 must be 64 hex characters", self.id);
        }
        Locator::parse(&self.manifest_url)
            .with_context(|| format!("plugin '{}' manifest_url", self.id))?;
        if let Some(url) = &self.entry_url {
            Locator::parse(url).with_context(|| format!("plugin '{}' entry_url", self.id))?;
        }
        let mut seen: HashSet<String> = HashSet::new();
        for file in &self.files {
            validate_relative_path(&file.path)
                .with_context(|| format!("plugin '{}' extra file", self.id))?;
            if file.path == MANIFEST_FILE || file.path == INSTALL_FILE {
                bail!(
                    "plugin '{}' ships '{}', which is reserved for the installer",
                    self.id,
                    file.path
                );
            }
            if file.sha256.len() != 64 || hex::decode(&file.sha256).is_err() {
                bail!(
                    "plugin '{}' file '{}' sha256 must be 64 hex characters",
                    self.id,
                    file.path
                );
            }
            Locator::parse(&file.url)
                .with_context(|| format!("plugin '{}' file '{}' url", self.id, file.path))?;
            if !seen.insert(file.path.clone()) {
                bail!("plugin '{}' lists '{}' twice", self.id, file.path);
            }
        }
        for dep in &self.dependencies {
            if !is_valid_id(&dep.id) {
                bail!("plugin '{}' depends on invalid id '{}'", self.id, dep.id);
            }
            if let Some(req) = &dep.version {
                Requirement::parse(req)
                    .with_context(|| format!("plugin '{}' dependency '{}'", self.id, dep.id))?;
            }
        }
        Ok(self)
    }
}

/// `index.json` of one market source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketIndex {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plugins: Vec<MarketEntry>,
}

/// An entry together with the source that published it.
#[derive(Debug, Clone)]
pub struct IndexedPlugin {
    pub entry: MarketEntry,
    pub source: String,
}

/// A source that could not be read — surfaced in the UI instead of failing the whole market.
#[derive(Debug, Clone, Serialize)]
pub struct MarketSourceError {
    pub source: String,
    pub error: String,
}

/// Everything a GET /api/market request returns.
#[derive(Debug, Clone, Serialize)]
pub struct MarketView {
    pub sources: Vec<String>,
    pub plugins: Vec<MarketPluginView>,
    pub errors: Vec<MarketSourceError>,
}

/// One row of the market list, annotated with the local install state.
#[derive(Debug, Clone, Serialize)]
pub struct MarketPluginView {
    #[serde(flatten)]
    pub entry: MarketEntry,
    /// The source this version came from.
    pub source: String,
    /// Locally installed version of the same id, when there is one.
    pub installed_version: Option<String>,
    /// The index has a strictly newer version than the installed one.
    pub update_available: bool,
}

/// Aggregated index of every configured source (highest version per id wins).
#[derive(Debug, Clone, Default)]
pub struct IndexSnapshot {
    pub plugins: Vec<IndexedPlugin>,
    pub errors: Vec<MarketSourceError>,
}

// ---------------------------------------------------------------------------
// Locations (one abstraction for http and local paths)
// ---------------------------------------------------------------------------

/// Where an index, manifest or wasm file lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Http(String),
    File(PathBuf),
}

impl Locator {
    /// Parse `https://…`, `http://…`, `file://…` or a plain path.
    ///
    /// Relative paths and `~` are resolved against the config dir, so
    /// `"sources": ["plugin-market/index.json"]` works the way people expect.
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            bail!("location must not be empty");
        }
        if raw.starts_with("http://") || raw.starts_with("https://") {
            return Ok(Self::Http(raw.to_string()));
        }
        let path = match raw.strip_prefix("file://") {
            Some(rest) => expand_home(Path::new(rest)),
            None => expand_home(Path::new(raw)),
        };
        let path = if path.is_absolute() {
            path
        } else {
            config::config_dir().join(path)
        };
        Ok(Self::File(path))
    }

    /// Resolve a reference relative to this location (a sibling file, or a relative URL).
    pub fn join(&self, reference: &str) -> Result<Self> {
        let reference = reference.trim();
        if reference.starts_with("http://")
            || reference.starts_with("https://")
            || reference.starts_with("file://")
        {
            return Self::parse(reference);
        }
        match self {
            Self::Http(base) => {
                let base = url::Url::parse(base).context("market source is not a valid URL")?;
                let joined = base
                    .join(reference)
                    .with_context(|| format!("cannot resolve '{reference}' against '{base}'"))?;
                Ok(Self::Http(joined.to_string()))
            }
            Self::File(path) => {
                let parent = path.parent().unwrap_or_else(|| Path::new("."));
                let joined = parent.join(reference);
                if joined.is_absolute() {
                    Ok(Self::File(joined))
                } else {
                    Ok(Self::File(config::config_dir().join(joined)))
                }
            }
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Http(url) => url.clone(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

fn expand_home(path: &Path) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path.to_path_buf();
    };
    if raw != "~" && !raw.starts_with("~/") && !raw.starts_with("~\\") {
        return path.to_path_buf();
    }
    match dirs::home_dir() {
        Some(home) if raw == "~" => home,
        Some(home) => home.join(raw.trim_start_matches("~").trim_start_matches(['/', '\\'])),
        None => path.to_path_buf(),
    }
}

struct Fetcher {
    client: reqwest::Client,
    timeout: Duration,
}

impl Fetcher {
    fn new(timeout_secs: u64) -> Self {
        let timeout = Duration::from_secs(timeout_secs.clamp(1, 600));
        Self {
            client: build_reqwest_client_with_timeout(timeout),
            timeout,
        }
    }

    async fn fetch(&self, locator: &Locator, limit: usize) -> Result<Vec<u8>> {
        let at = locator.display();
        match locator {
            Locator::File(path) => {
                let meta =
                    std::fs::metadata(path).with_context(|| format!("{at} is not readable"))?;
                if meta.len() > limit as u64 {
                    bail!("{at} is {} bytes (limit {limit})", meta.len());
                }
                std::fs::read(path).with_context(|| format!("read {at}"))
            }
            Locator::Http(url) => {
                let response = self
                    .client
                    .get(url)
                    .timeout(self.timeout)
                    .send()
                    .await
                    .with_context(|| format!("GET {url} failed"))?;
                let status = response.status();
                if !status.is_success() {
                    bail!("GET {url} returned HTTP {status}");
                }
                if let Some(len) = response.content_length() {
                    if len > limit as u64 {
                        bail!("GET {url} announced {len} bytes (limit {limit})");
                    }
                }
                let bytes = response
                    .bytes()
                    .await
                    .with_context(|| format!("read body of {url}"))?;
                if bytes.len() > limit {
                    bail!("GET {url} returned {} bytes (limit {limit})", bytes.len());
                }
                Ok(bytes.to_vec())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reading and merging indexes
// ---------------------------------------------------------------------------

/// Read every configured source and merge them (highest version per id wins).
///
/// A broken source is reported in [`IndexSnapshot::errors`] instead of hiding the working ones.
pub async fn fetch_index(sources: &[String], timeout_secs: u64) -> IndexSnapshot {
    let fetcher = Fetcher::new(timeout_secs);
    let mut best: BTreeMap<String, IndexedPlugin> = BTreeMap::new();
    let mut errors = Vec::new();

    for raw in sources {
        let locator = match Locator::parse(raw) {
            Ok(locator) => locator,
            Err(e) => {
                errors.push(MarketSourceError {
                    source: raw.clone(),
                    error: format!("{e:#}"),
                });
                continue;
            }
        };
        let text = match fetcher.fetch(&locator, MAX_INDEX_BYTES).await {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    errors.push(MarketSourceError {
                        source: raw.clone(),
                        error: "index is not valid UTF-8".to_string(),
                    });
                    continue;
                }
            },
            Err(e) => {
                errors.push(MarketSourceError {
                    source: raw.clone(),
                    error: format!("{e:#}"),
                });
                continue;
            }
        };
        let index: MarketIndex = match serde_json::from_str(&text) {
            Ok(index) => index,
            Err(e) => {
                errors.push(MarketSourceError {
                    source: raw.clone(),
                    error: format!("index is not valid JSON: {e}"),
                });
                continue;
            }
        };
        for entry in index.plugins {
            match entry.prepare() {
                Ok(entry) => {
                    let version = Version::parse(&entry.version).expect("validated above");
                    let replace = match best.get(&entry.id) {
                        Some(existing) => {
                            let current = Version::parse(&existing.entry.version)
                                .unwrap_or_else(|_| version.clone());
                            version > current
                        }
                        None => true,
                    };
                    if replace {
                        best.insert(
                            entry.id.clone(),
                            IndexedPlugin {
                                entry,
                                source: raw.clone(),
                            },
                        );
                    }
                }
                Err(e) => errors.push(MarketSourceError {
                    source: raw.clone(),
                    error: format!("{e:#}"),
                }),
            }
        }
    }

    IndexSnapshot {
        plugins: best.into_values().collect(),
        errors,
    }
}

/// Installed `id → version` map used to decide what is missing or outdated.
pub fn installed_versions(plugins: &[InstalledPlugin]) -> HashMap<String, String> {
    plugins
        .iter()
        .map(|p| (p.id.clone(), p.version.clone()))
        .collect()
}

/// Market list for the UI, annotated with the local install state.
pub async fn view(
    sources: &[String],
    timeout_secs: u64,
    installed: &HashMap<String, String>,
) -> MarketView {
    let snapshot = fetch_index(sources, timeout_secs).await;
    let plugins = snapshot
        .plugins
        .into_iter()
        .map(|indexed| {
            let installed_version = installed.get(&indexed.entry.id).cloned();
            let update_available = match &installed_version {
                Some(raw) => match (Version::parse(raw), Version::parse(&indexed.entry.version)) {
                    (Ok(have), Ok(offered)) => offered > have,
                    _ => false,
                },
                None => false,
            };
            MarketPluginView {
                entry: indexed.entry,
                source: indexed.source,
                installed_version,
                update_available,
            }
        })
        .collect();
    MarketView {
        sources: sources.to_vec(),
        plugins,
        errors: snapshot.errors,
    }
}

// ---------------------------------------------------------------------------
// Dependency resolution
// ---------------------------------------------------------------------------

/// Pick the versions to install for `id`, dependencies first, skipping what is already installed.
///
/// Dependencies are only skipped when the installed version satisfies the requirement; a cycle
/// terminates because every id is picked at most once.
pub fn resolve<'a>(
    entries: &'a [IndexedPlugin],
    id: &str,
    version: Option<&str>,
    installed: &HashMap<String, String>,
) -> Result<Vec<&'a MarketEntry>> {
    if !is_valid_id(id) {
        bail!("invalid plugin id '{id}'");
    }
    let root_req = match version {
        Some(raw) => Some(Requirement::parse(&format!("={}", raw.trim()))?),
        None => None,
    };

    let mut queue: VecDeque<(String, Option<Requirement>, bool)> =
        VecDeque::from([(id.to_string(), root_req, true)]);
    let mut picked: Vec<&MarketEntry> = Vec::new();
    let mut picked_index: HashMap<String, usize> = HashMap::new();

    while let Some((dep_id, requirement, explicit)) = queue.pop_front() {
        if let Some(&index) = picked_index.get(&dep_id) {
            if let Some(requirement) = &requirement {
                let chosen = picked[index];
                let chosen_version = Version::parse(&chosen.version)?;
                if !requirement.matches(&chosen_version) {
                    bail!(
                        "dependency conflict: '{}' was resolved to {} but is also required as {}",
                        dep_id,
                        chosen.version,
                        requirement
                    );
                }
            }
            continue;
        }
        if !explicit {
            if let Some(raw) = installed.get(&dep_id) {
                let satisfied = match &requirement {
                    Some(requirement) => Version::parse(raw)
                        .map(|v| requirement.matches(&v))
                        .unwrap_or(false),
                    None => true,
                };
                if satisfied {
                    continue;
                }
            }
        }
        let chosen = best_match(entries, &dep_id, requirement.as_ref())?;
        picked_index.insert(dep_id.clone(), picked.len());
        picked.push(chosen);
        for dep in &chosen.dependencies {
            let requirement = match &dep.version {
                Some(raw) => Some(Requirement::parse(raw)?),
                None => None,
            };
            queue.push_back((dep.id.clone(), requirement, false));
        }
    }

    Ok(order_dependencies_first(picked))
}

fn best_match<'a>(
    entries: &'a [IndexedPlugin],
    id: &str,
    requirement: Option<&Requirement>,
) -> Result<&'a MarketEntry> {
    let mut best: Option<(Version, &MarketEntry)> = None;
    for entry in entries.iter().filter(|p| p.entry.id == id) {
        let Ok(version) = Version::parse(&entry.entry.version) else {
            continue;
        };
        if let Some(requirement) = requirement {
            if !requirement.matches(&version) {
                continue;
            }
        }
        if best
            .as_ref()
            .map(|(seen, _)| version > *seen)
            .unwrap_or(true)
        {
            best = Some((version, &entry.entry));
        }
    }
    match best {
        Some((_, entry)) => Ok(entry),
        None => match requirement {
            Some(requirement) => bail!("no version of '{id}' satisfies {requirement}"),
            None => bail!("plugin '{id}' is not available from any market source"),
        },
    }
}

/// Post-order DFS so a dependency is installed before the package that needs it.
fn order_dependencies_first(picked: Vec<&MarketEntry>) -> Vec<&MarketEntry> {
    let by_id: HashMap<&str, &MarketEntry> = picked.iter().map(|e| (e.id.as_str(), *e)).collect();
    let mut ordered: Vec<&MarketEntry> = Vec::with_capacity(picked.len());
    let mut done: HashSet<&str> = HashSet::new();
    for entry in &picked {
        visit(entry, &by_id, &mut done, &mut ordered);
    }
    ordered
}

fn visit<'a>(
    entry: &'a MarketEntry,
    by_id: &HashMap<&'a str, &'a MarketEntry>,
    done: &mut HashSet<&'a str>,
    ordered: &mut Vec<&'a MarketEntry>,
) {
    if done.contains(entry.id.as_str()) {
        return;
    }
    // Mark before recursing so a cycle (a → b → a) cannot loop.
    done.insert(entry.id.as_str());
    for dep in &entry.dependencies {
        if let Some(next) = by_id.get(dep.id.as_str()) {
            visit(next, by_id, done, ordered);
        }
    }
    ordered.push(entry);
}

// ---------------------------------------------------------------------------
// Download, verify, install
// ---------------------------------------------------------------------------

/// A package downloaded and verified into a staging directory, ready for the installer.
#[derive(Debug)]
pub struct StagedPackage {
    pub entry: MarketEntry,
    pub dir: PathBuf,
}

/// One plugin that was installed by this request.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledSummary {
    pub id: String,
    pub version: String,
    pub trust: Trust,
    pub dependencies: Vec<String>,
}

/// One plugin that failed to install.
#[derive(Debug, Clone, Serialize)]
pub struct MarketFailure {
    pub id: String,
    pub error: String,
}

/// Result of an install/update request: partial success is reported, not thrown away.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MarketInstallReport {
    pub installed: Vec<InstalledSummary>,
    pub failed: Vec<MarketFailure>,
}

impl MarketInstallReport {
    pub fn is_empty(&self) -> bool {
        self.installed.is_empty()
    }
}

/// Resolve `id` (plus dependencies) against every source and download the packages.
///
/// `plugins_root` is where the staging directories go (same filesystem as the real packages).
pub async fn fetch_packages(
    sources: &[String],
    timeout_secs: u64,
    plugins_root: &Path,
    id: &str,
    version: Option<&str>,
    installed: &HashMap<String, String>,
) -> Result<Vec<StagedPackage>> {
    let snapshot = fetch_index(sources, timeout_secs).await;
    let plan = resolve(&snapshot.plugins, id, version, installed)?;
    let fetcher = Fetcher::new(timeout_secs);
    stage_all(&fetcher, plugins_root, plan).await
}

/// Download every package that is newer than the installed copy.
pub async fn fetch_updates(
    sources: &[String],
    timeout_secs: u64,
    plugins_root: &Path,
    installed: &HashMap<String, String>,
) -> Result<Vec<StagedPackage>> {
    let snapshot = fetch_index(sources, timeout_secs).await;
    let mut targets: Vec<&MarketEntry> = Vec::new();
    for indexed in &snapshot.plugins {
        let Some(raw) = installed.get(&indexed.entry.id) else {
            continue;
        };
        let newer = match (Version::parse(raw), Version::parse(&indexed.entry.version)) {
            (Ok(have), Ok(offered)) => offered > have,
            _ => false,
        };
        if !newer {
            continue;
        }
        // A dependency of one target may also be a target: dedupe by id.
        if targets.iter().any(|t| t.id == indexed.entry.id) {
            continue;
        }
        targets.push(&indexed.entry);
    }
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    // Run every target through the resolver again so a dependency that is missing (or too old)
    // comes along with the update instead of failing the install later.
    let mut plan: Vec<&MarketEntry> = Vec::new();
    for target in targets {
        for entry in resolve(
            &snapshot.plugins,
            &target.id,
            Some(&target.version),
            installed,
        )? {
            if !plan.iter().any(|picked| picked.id == entry.id) {
                plan.push(entry);
            }
        }
    }
    let fetcher = Fetcher::new(timeout_secs);
    stage_all(&fetcher, plugins_root, plan).await
}

async fn stage_all(
    fetcher: &Fetcher,
    plugins_root: &Path,
    plan: Vec<&MarketEntry>,
) -> Result<Vec<StagedPackage>> {
    let mut staged = Vec::new();
    for entry in plan {
        match fetch_package(fetcher, plugins_root, entry).await {
            Ok(package) => staged.push(package),
            Err(e) => {
                cleanup(&staged);
                return Err(e);
            }
        }
    }
    Ok(staged)
}

async fn fetch_package(
    fetcher: &Fetcher,
    plugins_root: &Path,
    entry: &MarketEntry,
) -> Result<StagedPackage> {
    let manifest_locator = Locator::parse(&entry.manifest_url)?;
    let manifest_bytes = fetcher
        .fetch(&manifest_locator, MAX_MANIFEST_BYTES)
        .await
        .with_context(|| format!("downloading the manifest of '{}'", entry.id))?;
    let manifest_text = String::from_utf8(manifest_bytes)
        .with_context(|| format!("manifest of '{}' is not UTF-8", entry.id))?;
    let manifest = PluginManifest::parse(&manifest_text)
        .with_context(|| format!("manifest of '{}'", entry.id))?;
    if manifest.id != entry.id {
        bail!(
            "index publishes '{}' but its plugin.json says '{}'",
            entry.id,
            manifest.id
        );
    }
    if manifest.version != entry.version {
        bail!(
            "index publishes {} {} but its plugin.json says {}",
            entry.id,
            entry.version,
            manifest.version
        );
    }

    let entry_locator = match &entry.entry_url {
        Some(raw) => Locator::parse(raw)?,
        None => manifest_locator.join(&manifest.entry)?,
    };
    let wasm = fetcher
        .fetch(&entry_locator, MAX_FILE_BYTES)
        .await
        .with_context(|| format!("downloading '{}@{}'", entry.id, entry.version))?;
    let digest = sha256_hex(&wasm);
    if !digest.eq_ignore_ascii_case(&entry.sha256) {
        bail!(
            "'{}@{}' entry sha256 mismatch: index says {}, downloaded {}",
            entry.id,
            entry.version,
            entry.sha256,
            digest
        );
    }

    let dir = staging_dir(plugins_root, &entry.id)?;
    let staged = async {
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        // Keep the published manifest byte for byte: it carries the signature we must verify.
        std::fs::write(dir.join(MANIFEST_FILE), manifest_text.as_bytes())?;
        let entry_path = dir.join(&manifest.entry);
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&entry_path, &wasm)?;

        for file in &entry.files {
            if file.path == manifest.entry {
                bail!(
                    "'{}' lists '{}' both as entry and as an extra file",
                    entry.id,
                    file.path
                );
            }
            let locator = Locator::parse(&file.url)?;
            let data = fetcher
                .fetch(&locator, MAX_FILE_BYTES)
                .await
                .with_context(|| format!("downloading '{}' of '{}'", file.path, entry.id))?;
            let got = sha256_hex(&data);
            if !got.eq_ignore_ascii_case(&file.sha256) {
                bail!(
                    "'{}' of '{}@{}' sha256 mismatch: index says {}, downloaded {}",
                    file.path,
                    entry.id,
                    entry.version,
                    file.sha256,
                    got
                );
            }
            let target = dir.join(&file.path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, data)?;
        }
        Ok(())
    }
    .await;

    match staged {
        Ok(()) => Ok(StagedPackage {
            entry: entry.clone(),
            dir,
        }),
        Err(e) => {
            std::fs::remove_dir_all(&dir).ok();
            Err(e)
        }
    }
}

fn staging_dir(plugins_root: &Path, id: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(plugins_root)
        .with_context(|| format!("create plugins dir {}", plugins_root.display()))?;
    Ok(plugins_root.join(format!(".market-{id}-{}", unique_suffix())))
}

fn cleanup(staged: &[StagedPackage]) {
    for package in staged {
        std::fs::remove_dir_all(&package.dir).ok();
    }
}

/// Install staged packages (dependencies first) and always remove the staging directories.
///
/// Failures do not abort the batch: the report lists what went in and what did not, so a broken
/// dependency cannot silently discard the packages that were fine.
pub fn install_staged(
    root: &Path,
    trust_path: &Path,
    staged: Vec<StagedPackage>,
) -> MarketInstallReport {
    let mut report = MarketInstallReport::default();
    for package in &staged {
        match install::install_from_dir(root, &package.dir, trust_path) {
            Ok(plugin) => report.installed.push(InstalledSummary {
                id: plugin.id,
                version: plugin.version,
                trust: plugin.trust,
                dependencies: package
                    .entry
                    .dependencies
                    .iter()
                    .map(|d| d.id.clone())
                    .collect(),
            }),
            Err(e) => report.failed.push(MarketFailure {
                id: package.entry.id.clone(),
                error: format!("{e:#}"),
            }),
        }
    }
    cleanup(&staged);
    report
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

    // ---- fixtures ---------------------------------------------------------

    /// Write `<root>/pkg/<id>-<version>/plugin.json` + wasm. Returns the manifest path + digest.
    fn write_package(root: &Path, id: &str, version: &str, wasm: &[u8]) -> (PathBuf, String) {
        let dir = root.join(format!("pkg/{id}-{version}"));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "id": id,
            "name": id,
            "version": version,
            "entry": format!("{id}.wasm"),
        });
        std::fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(format!("{id}.wasm")), wasm).unwrap();
        (dir.join(MANIFEST_FILE), sha256_hex(wasm))
    }

    fn market_entry(
        id: &str,
        version: &str,
        manifest: &Path,
        sha: &str,
        deps: &[(&str, &str)],
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": format!("{id} display"),
            "version": version,
            "manifest_url": manifest.display().to_string(),
            "sha256": sha,
            "dependencies": deps
                .iter()
                .map(|(id, version)| serde_json::json!({ "id": id, "version": version }))
                .collect::<Vec<_>>(),
        })
    }

    fn write_index(root: &Path, name: &str, entries: Vec<serde_json::Value>) -> String {
        let path = root.join(format!("{name}-index.json"));
        let index = serde_json::json!({ "name": name, "plugins": entries });
        std::fs::write(&path, serde_json::to_string_pretty(&index).unwrap()).unwrap();
        path.display().to_string()
    }

    fn indexed(entry: MarketEntry) -> IndexedPlugin {
        IndexedPlugin {
            entry,
            source: "test".to_string(),
        }
    }

    /// Index entry parsed from JSON and normalised, without touching the filesystem.
    fn entry(id: &str, version: &str, deps: &[(&str, &str)]) -> MarketEntry {
        let value = serde_json::json!({
            "id": id,
            "version": version,
            "manifest_url": format!("/tmp/{id}-{version}/plugin.json"),
            "sha256": "a".repeat(64),
            "dependencies": deps
                .iter()
                .map(|(id, version)| serde_json::json!({ "id": id, "version": version }))
                .collect::<Vec<_>>(),
        });
        serde_json::from_value::<MarketEntry>(value)
            .unwrap()
            .prepare()
            .unwrap()
    }

    // ---- versions ---------------------------------------------------------

    #[test]
    fn versions_order_and_prereleases_sort_below_releases() {
        let v = |s: &str| Version::parse(s).unwrap();
        assert_eq!(v("1.2"), v("1.2.0"));
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(v("2.0.0") > v("1.99.99"));
        assert!(v("1.0.0-alpha") < v("1.0.0"));
        assert!(v("1.0.0-alpha.2") > v("1.0.0-alpha.1"));
        assert_eq!(v("v1.2.3+build.5").to_string(), "1.2.3");
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("x.y.z").is_err());
    }

    #[test]
    fn requirements_cover_the_usual_operators() {
        let v = |s: &str| Version::parse(s).unwrap();
        let m = |req: &str, version: &str| Requirement::parse(req).unwrap().matches(&v(version));
        assert!(m("*", "9.9.9"));
        assert!(m("1.2.3", "1.2.3") && !m("1.2.3", "1.2.4"));
        assert!(m(">=1.0.0 <2.0.0", "1.9.0") && !m(">=1.0.0 <2.0.0", "2.0.0"));
        assert!(m("^1.2.0", "1.9.9") && !m("^1.2.0", "2.0.0"));
        assert!(m("^0.2.0", "0.2.9") && !m("^0.2.0", "0.3.0"));
        assert!(m("~1.2.0", "1.2.9") && !m("~1.2.0", "1.3.0"));
        assert!(m(">1.0.0", "1.0.1") && !m(">1.0.0", "1.0.0"));
        assert!(Requirement::parse(">=").is_err());
        assert!(Requirement::parse("abc").is_err());
    }

    // ---- indexes ----------------------------------------------------------

    #[tokio::test]
    async fn index_merges_sources_and_keeps_the_highest_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (older_manifest, older_sha) = write_package(root, "demo", "1.0.0", b"\0asm\x01");
        let (newer_manifest, newer_sha) = write_package(root, "demo", "1.1.0", b"\0asm\x02");
        let older = write_index(
            root,
            "older",
            vec![market_entry(
                "demo",
                "1.0.0",
                &older_manifest,
                &older_sha,
                &[],
            )],
        );
        let newer = write_index(
            root,
            "newer",
            vec![market_entry(
                "demo",
                "1.1.0",
                &newer_manifest,
                &newer_sha,
                &[],
            )],
        );

        let snapshot = fetch_index(&[older, newer.clone()], 5).await;
        assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.plugins[0].entry.version, "1.1.0");
        assert_eq!(snapshot.plugins[0].source, newer);
    }

    #[tokio::test]
    async fn broken_sources_are_reported_without_hiding_working_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (manifest, sha) = write_package(root, "demo", "1.0.0", b"\0asm\x01");
        let good = write_index(
            root,
            "good",
            vec![market_entry("demo", "1.0.0", &manifest, &sha, &[])],
        );
        let missing = root.join("nope/index.json").display().to_string();
        let garbage = root.join("garbage-index.json");
        std::fs::write(&garbage, "not json").unwrap();

        let snapshot = fetch_index(&[good, missing, garbage.display().to_string()], 5).await;
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.errors.len(), 2);
    }

    #[test]
    fn entries_with_escaping_extra_files_are_rejected() {
        let value = serde_json::json!({
            "id": "app",
            "version": "1.0.0",
            "manifest_url": "/tmp/app/plugin.json",
            "sha256": "a".repeat(64),
            "files": [{ "path": "../escape.txt", "url": "/tmp/escape.txt", "sha256": "b".repeat(64) }],
        });
        let entry: MarketEntry = serde_json::from_value(value).unwrap();
        let err = entry.prepare().unwrap_err().to_string();
        assert!(err.contains("extra file"), "{err}");
    }

    // ---- resolution -------------------------------------------------------

    #[test]
    fn resolution_puts_dependencies_first_and_skips_satisfied_ones() {
        let entries = vec![
            indexed(entry("app", "1.2.0", &[("base", ">=1.0.0")])),
            indexed(entry("base", "1.0.0", &[])),
            indexed(entry("base", "1.1.0", &[])),
        ];
        let plan = resolve(&entries, "app", None, &HashMap::new()).unwrap();
        let ids: Vec<&str> = plan.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["base", "app"]);
        assert_eq!(plan[0].version, "1.1.0");

        let installed = HashMap::from([("base".to_string(), "1.1.0".to_string())]);
        let plan = resolve(&entries, "app", None, &installed).unwrap();
        assert_eq!(
            plan.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["app"]
        );

        // An installed version that does not satisfy the requirement is upgraded.
        let installed = HashMap::from([("base".to_string(), "0.9.0".to_string())]);
        let plan = resolve(&entries, "app", None, &installed).unwrap();
        assert_eq!(
            plan.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["base", "app"]
        );
    }

    #[test]
    fn resolution_reports_missing_plugins_and_version_conflicts() {
        let entries = vec![
            indexed(entry("app", "1.0.0", &[("base", ">=2.0.0")])),
            indexed(entry("base", "1.0.0", &[])),
        ];
        let err = resolve(&entries, "app", None, &HashMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no version of 'base'"), "{err}");

        let entries = vec![
            indexed(entry("app", "1.0.0", &[("base", "^1.0.0"), ("lib", "*")])),
            indexed(entry("lib", "1.0.0", &[("base", "^2.0.0")])),
            indexed(entry("base", "1.0.0", &[])),
            indexed(entry("base", "2.0.0", &[])),
        ];
        let err = resolve(&entries, "app", None, &HashMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("dependency conflict"), "{err}");

        let err = resolve(&entries, "ghost", None, &HashMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("ghost"), "{err}");
    }

    #[test]
    fn dependency_cycles_terminate() {
        let entries = vec![
            indexed(entry("a", "1.0.0", &[("b", "*")])),
            indexed(entry("b", "1.0.0", &[("a", "*")])),
        ];
        let plan = resolve(&entries, "a", None, &HashMap::new()).unwrap();
        assert_eq!(plan.len(), 2);
    }

    // ---- install ----------------------------------------------------------

    #[tokio::test]
    async fn install_downloads_verifies_and_leaves_no_staging_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (base_manifest, base_sha) = write_package(root, "base", "1.0.0", b"\0asm\x01");
        let (app_manifest, app_sha) = write_package(root, "app", "1.2.0", b"\0asm\x02");
        let index = write_index(
            root,
            "market",
            vec![
                market_entry(
                    "app",
                    "1.2.0",
                    &app_manifest,
                    &app_sha,
                    &[("base", ">=1.0.0")],
                ),
                market_entry("base", "1.0.0", &base_manifest, &base_sha, &[]),
            ],
        );

        let plugins = root.join("plugins");
        let trust = root.join("plugin_keys.json");
        let staged = fetch_packages(&[index], 5, &plugins, "app", None, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(staged.len(), 2);

        let report = install_staged(&plugins, &trust, staged);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(
            report
                .installed
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            vec!["base", "app"]
        );

        let listed = install::list(&plugins, &HashMap::new());
        assert_eq!(listed.len(), 2);
        let app = listed
            .iter()
            .find(|p| p.id == "app")
            .expect("app installed");
        assert_eq!(app.version, "1.2.0");
        assert!(app.loadable);
        let leftovers: Vec<String> = std::fs::read_dir(&plugins)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            leftovers.len(),
            2,
            "staging dirs must be cleaned: {leftovers:?}"
        );
    }

    #[tokio::test]
    async fn a_digest_mismatch_aborts_before_anything_is_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (manifest, _) = write_package(root, "app", "1.0.0", b"\0asm\x01");
        let index = write_index(
            root,
            "market",
            vec![market_entry(
                "app",
                "1.0.0",
                &manifest,
                &"0".repeat(64),
                &[],
            )],
        );
        let plugins = root.join("plugins");
        let err = fetch_packages(&[index], 5, &plugins, "app", None, &HashMap::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("sha256 mismatch"), "{err}");
        let leftovers = std::fs::read_dir(&plugins).map(|d| d.count()).unwrap_or(0);
        assert_eq!(leftovers, 0, "nothing may be staged or installed");
    }

    #[tokio::test]
    async fn the_index_cannot_lie_about_the_manifest_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (manifest, sha) = write_package(root, "app", "1.0.0", b"\0asm\x01");
        let index = write_index(
            root,
            "market",
            vec![market_entry("app", "1.1.0", &manifest, &sha, &[])],
        );
        let plugins = root.join("plugins");
        let err = fetch_packages(&[index], 5, &plugins, "app", None, &HashMap::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("plugin.json says"), "{err}");
    }

    #[tokio::test]
    async fn updates_pick_up_newer_versions_and_their_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (base_manifest, base_sha) = write_package(root, "base", "1.0.0", b"\0asm\x01");
        let (app_manifest, app_sha) = write_package(root, "app", "1.1.0", b"\0asm\x02");
        let index = write_index(
            root,
            "market",
            vec![
                market_entry("app", "1.1.0", &app_manifest, &app_sha, &[("base", "*")]),
                market_entry("base", "1.0.0", &base_manifest, &base_sha, &[]),
            ],
        );
        let installed = HashMap::from([("app".to_string(), "1.0.0".to_string())]);
        let plugins = root.join("plugins");

        let view = view(std::slice::from_ref(&index), 5, &installed).await;
        assert_eq!(view.plugins[0].installed_version.as_deref(), Some("1.0.0"));
        assert!(view.plugins[0].update_available);

        let staged = fetch_updates(&[index], 5, &plugins, &installed)
            .await
            .unwrap();
        let mut ids: Vec<&str> = staged.iter().map(|p| p.entry.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["app", "base"]);
        for package in &staged {
            std::fs::remove_dir_all(&package.dir).ok();
        }
    }
}
