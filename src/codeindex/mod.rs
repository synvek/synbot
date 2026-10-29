//! Repository symbol index — the map an agent reads before it starts grepping.
//!
//! One index per workspace, persisted as `<workspace>/.synbot/index/code.json`. Building it is a
//! single line scan per file: no tree-sitter grammars, because the caller is an LLM with
//! `read_file` and what it needs is "where is `foo` defined, in what kind of item, on which line".
//! A per-language scanner answers that within a few lines of the truth, while grammars would add
//! tens of megabytes of C build steps to the crate.
//!
//! Refreshes are incremental: every file stores its `(mtime, size)` fingerprint, so a refresh only
//! re-parses what actually changed and drops what disappeared. Files, sizes and symbol counts are
//! capped so a runaway tree cannot stall a turn.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the index lives, relative to the workspace root.
pub const INDEX_DIR: &str = ".synbot/index";
/// Generated markdown, relative to the workspace root.
pub const WIKI_DIR: &str = ".synbot/repowiki";
/// Bumped when the on-disk shape changes; older files are rebuilt instead of misread.
pub const INDEX_VERSION: u32 = 1;
/// Files indexed per workspace.
pub const MAX_FILES: usize = 4000;
/// Files larger than this are skipped (generated bundles, vendored blobs).
pub const MAX_FILE_BYTES: u64 = 512 * 1024;
/// Symbols kept per file (a machine-generated file can have thousands of one-liners).
pub const MAX_SYMBOLS_PER_FILE: usize = 400;
/// Hits returned by one search.
pub const MAX_HITS: usize = 50;
/// Module pages written by `write_wiki`.
pub const MAX_WIKI_MODULES: usize = 20;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".synbot",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "bin",
    "obj",
    "coverage",
    "vendor",
    // Vendored/third-party trees are not "this" repository: indexing them floods the map with
    // symbols nobody on the team can change (and they are usually the bulk of the files).
    "third_party",
    "third-party",
    "external",
    "deps",
    "site-packages",
    "fixtures",
    "test-fixtures",
    "Pods",
    "venv",
    "__pycache__",
    ".venv",
    ".tox",
    ".next",
    ".nuxt",
    ".turbo",
    ".gradle",
    ".idea",
    ".vscode",
];

/// Files that tend to be entry points; the wiki lists their top symbols first.
const ENTRY_FILES: &[&str] = &[
    "src/main.rs",
    "src/lib.rs",
    "main.rs",
    "lib.rs",
    "main.py",
    "app.py",
    "manage.py",
    "main.go",
    "Program.cs",
    "index.ts",
    "index.js",
    "src/index.ts",
    "src/main.ts",
    "src/main.tsx",
    "src/App.tsx",
    "src/app/page.tsx",
];

/// Source languages the scanner understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Go,
    Java,
    Kotlin,
    Swift,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
}

impl Lang {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".min.js") || name.ends_with(".min.css") || name.ends_with(".lock") {
            return None;
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "rs" => Self::Rust,
            "py" | "pyi" => Self::Python,
            "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "jsx" => Self::Jsx,
            "go" => Self::Go,
            "java" => Self::Java,
            "kt" | "kts" => Self::Kotlin,
            "swift" => Self::Swift,
            "c" | "h" => Self::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Self::Cpp,
            "cs" => Self::CSharp,
            "rb" => Self::Ruby,
            "php" => Self::Php,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::Go => "go",
            Self::Java => "java",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Php => "php",
        }
    }
}

/// One definition found in a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    /// `fn` / `method` / `struct` / `enum` / `trait` / `impl` / `mod` / `class` / `interface` /
    /// `type` / `const` / `func` / `def` — coarse on purpose.
    pub kind: String,
    /// 1-based.
    pub line: usize,
    /// 1-based column of the definition (indentation + 1).
    pub column: usize,
    /// Enclosing `impl` / class / trait / module name, when the language or indentation shows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// First line of the definition, trimmed and capped — enough to recognise the signature.
    pub signature: String,
}

/// One indexed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to the workspace, always with `/` separators.
    pub path: String,
    pub lang: String,
    /// Fingerprint part 1: modification time in milliseconds since the epoch.
    pub mtime: u64,
    /// Fingerprint part 2: size in bytes.
    pub size: u64,
    #[serde(default)]
    pub symbols: Vec<Symbol>,
}

/// The persisted index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIndex {
    pub version: u32,
    pub root: String,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
}

/// Outcome of a rebuild, also returned by the tool and the REST endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    /// Files read and re-scanned in this refresh.
    pub parsed: usize,
    /// Files whose fingerprint matched and were carried over untouched.
    pub reused: usize,
    /// Files that disappeared since the previous refresh.
    pub removed: usize,
    /// The walk hit [`MAX_FILES`] and stopped.
    pub truncated: bool,
    pub path: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub languages: BTreeMap<String, usize>,
}

/// Size per top-level folder: the shape of the wiki and of the prompt's repo map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleSummary {
    pub name: String,
    pub files: usize,
    pub symbols: usize,
}

/// Small sidecar (`summary.json`) so prompt building never parses the whole index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub root: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub files: usize,
    pub symbols: usize,
    pub languages: BTreeMap<String, usize>,
    pub modules: Vec<ModuleSummary>,
    pub path: String,
}

/// One ranked search hit.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    pub signature: String,
    pub score: u32,
}

/// What `write_wiki` produced.
#[derive(Debug, Clone, Serialize)]
pub struct WikiReport {
    pub dir: String,
    /// Paths relative to `dir`.
    pub files: Vec<String>,
    pub modules: usize,
    /// More modules existed than [`MAX_WIKI_MODULES`].
    pub truncated: bool,
}

impl CodeIndex {
    pub fn empty(root: &Path) -> Self {
        Self {
            version: INDEX_VERSION,
            root: root.to_string_lossy().into_owned(),
            updated_at: None,
            files: Vec::new(),
        }
    }

    pub fn path_for(root: &Path) -> PathBuf {
        root.join(INDEX_DIR).join("code.json")
    }

    pub fn summary_path_for(root: &Path) -> PathBuf {
        root.join(INDEX_DIR).join("summary.json")
    }

    /// True when an index file exists (its freshness is the caller's problem).
    pub fn is_built(root: &Path) -> bool {
        Self::path_for(root).is_file()
    }

    /// Read the index; a missing or damaged file yields an empty index rather than an error, so a
    /// bad JSON blob cannot break a turn (the next refresh overwrites it).
    pub fn load(root: &Path) -> Self {
        let path = Self::path_for(root);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::empty(root),
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(index) if index.version == INDEX_VERSION => index,
            Ok(index) => {
                tracing::debug!(
                    path = %path.display(),
                    version = index.version,
                    "code index version changed, rebuilding"
                );
                Self::empty(root)
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "code index unreadable, rebuilding");
                Self::empty(root)
            }
        }
    }

    /// Atomic write (tmp + rename) so a crash mid-save cannot truncate the index.
    pub fn save(&self, root: &Path) -> Result<()> {
        let path = Self::path_for(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create index dir {}", dir.display()))?;
        }
        let raw = serde_json::to_vec(self).context("serialize code index")?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, raw).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
        self.save_summary(root)
    }

    /// Write the compact sidecar used by prompt building.
    pub fn save_summary(&self, root: &Path) -> Result<()> {
        let summary = self.summary();
        let path = Self::summary_path_for(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create index dir {}", dir.display()))?;
        }
        let raw = serde_json::to_vec_pretty(&summary).context("serialize index summary")?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, raw).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
        Ok(())
    }

    /// Walk the workspace, re-scanning only files whose fingerprint changed, then persist.
    pub fn refresh(&mut self, root: &Path) -> Result<IndexStats> {
        let previous: BTreeMap<String, FileEntry> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect();
        let mut remaining = previous;
        let mut next: Vec<FileEntry> = Vec::new();
        let mut parsed = 0usize;
        let mut reused = 0usize;
        let mut truncated = false;

        let mut pending_dirs = vec![root.to_path_buf()];
        'walk: while let Some(dir) = pending_dirs.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::debug!(dir = %dir.display(), error = %e, "skipping unreadable dir");
                    continue;
                }
            };
            for entry in entries.flatten() {
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => continue,
                };
                // Symlinks are skipped: they add cycles and escape the workspace without warning.
                if file_type.is_symlink() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if file_type.is_dir() {
                    if !is_ignored_dir(&name) {
                        pending_dirs.push(path);
                    }
                    continue;
                }
                let Some(lang) = Lang::from_path(&path) else {
                    continue;
                };
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.len() > MAX_FILE_BYTES {
                    continue;
                }
                let rel = relative(root, &path);
                let mtime = mtime_ms(&meta);
                let size = meta.len();
                if let Some(old) = remaining.remove(&rel) {
                    if old.mtime == mtime && old.size == size {
                        next.push(old);
                        reused += 1;
                        continue;
                    }
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    // Binary or non-UTF-8 despite the extension: not our business.
                    continue;
                };
                next.push(FileEntry {
                    path: rel,
                    lang: lang.as_str().to_string(),
                    mtime,
                    size,
                    symbols: extract_symbols(lang, &content),
                });
                parsed += 1;
                if next.len() >= MAX_FILES {
                    truncated = true;
                    break 'walk;
                }
            }
        }

        let removed = remaining.len();
        next.sort_by(|a, b| a.path.cmp(&b.path));
        self.files = next;
        self.version = INDEX_VERSION;
        self.root = root.to_string_lossy().into_owned();
        self.updated_at = Some(Utc::now());

        let stats = IndexStats {
            files: self.files.len(),
            symbols: self.symbol_count(),
            parsed,
            reused,
            removed,
            truncated,
            path: Self::path_for(root).display().to_string(),
            updated_at: self.updated_at,
            languages: self.language_counts(),
        };
        self.save(root)?;
        Ok(stats)
    }

    pub fn symbol_count(&self) -> usize {
        self.files.iter().map(|file| file.symbols.len()).sum()
    }

    /// Files per language.
    pub fn language_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for file in &self.files {
            *counts.entry(file.lang.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Files + symbols per top-level folder (root-level files land in `(root)`).
    pub fn modules(&self) -> Vec<ModuleSummary> {
        let mut modules: BTreeMap<String, ModuleSummary> = BTreeMap::new();
        for file in &self.files {
            let name = file
                .path
                .split_once('/')
                .map(|(head, _)| head)
                .unwrap_or("(root)")
                .to_string();
            let entry = modules.entry(name.clone()).or_insert(ModuleSummary {
                name,
                files: 0,
                symbols: 0,
            });
            entry.files += 1;
            entry.symbols += file.symbols.len();
        }
        let mut out: Vec<ModuleSummary> = modules.into_values().collect();
        out.sort_by(|a, b| {
            b.symbols
                .cmp(&a.symbols)
                .then(b.files.cmp(&a.files))
                .then(a.name.cmp(&b.name))
        });
        out
    }

    pub fn summary(&self) -> IndexSummary {
        IndexSummary {
            root: self.root.clone(),
            updated_at: self.updated_at,
            files: self.files.len(),
            symbols: self.symbol_count(),
            languages: self.language_counts(),
            modules: self.modules(),
            path: Self::path_for(Path::new(&self.root)).display().to_string(),
        }
    }

    /// Find symbols by name. `query` may carry a scope (`AgentLoop::run`, `AgentLoop.run`), in
    /// which case only symbols whose container matches are returned.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let (scope, name) = split_query(query);
        if name.is_empty() {
            return Vec::new();
        }
        let limit = limit.clamp(1, MAX_HITS);
        let mut hits = Vec::new();
        for file in &self.files {
            for symbol in &file.symbols {
                if let Some(score) = score_symbol(symbol, &name, scope) {
                    hits.push(Hit {
                        path: file.path.clone(),
                        line: symbol.line,
                        column: symbol.column,
                        kind: symbol.kind.clone(),
                        name: symbol.name.clone(),
                        container: symbol.container.clone(),
                        signature: symbol.signature.clone(),
                        score,
                    });
                }
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });
        hits.truncate(limit);
        hits
    }

    /// Symbols of a single file, matched by relative path.
    pub fn outline(&self, path: &str) -> Option<&FileEntry> {
        let needle = path.trim_start_matches("./").replace('\\', "/");
        self.files.iter().find(|file| file.path == needle)
    }

    /// Write `.synbot/repowiki/{index.md, modules/<dir>.md}`.
    pub fn write_wiki(&self, root: &Path) -> Result<WikiReport> {
        let dir = root.join(WIKI_DIR);
        let modules_dir = dir.join("modules");
        std::fs::create_dir_all(&modules_dir)
            .with_context(|| format!("create wiki dir {}", modules_dir.display()))?;

        let modules = self.modules();
        let mut files = vec!["index.md".to_string()];
        write_file(&dir.join("index.md"), &self.render_index(&modules))?;
        for module in modules.iter().take(MAX_WIKI_MODULES) {
            let slug = slug(&module.name);
            write_file(
                &modules_dir.join(format!("{slug}.md")),
                &self.render_module(&module.name),
            )?;
            files.push(format!("modules/{slug}.md"));
        }
        Ok(WikiReport {
            dir: dir.display().to_string(),
            files,
            modules: modules.len(),
            truncated: modules.len() > MAX_WIKI_MODULES,
        })
    }

    fn render_index(&self, modules: &[ModuleSummary]) -> String {
        let mut out = String::new();
        out.push_str("# Repository index\n\n");
        out.push_str(&format!("- Root: `{}`\n", self.root));
        out.push_str(&format!("- Files: {}\n", self.files.len()));
        out.push_str(&format!("- Symbols: {}\n", self.symbol_count()));
        if let Some(updated) = self.updated_at {
            out.push_str(&format!("- Updated: {}\n", updated.to_rfc3339()));
        }

        out.push_str("\n## Languages\n\n| Language | Files |\n| --- | --- |\n");
        let mut languages: BTreeMap<&str, usize> = BTreeMap::new();
        for file in &self.files {
            *languages.entry(file.lang.as_str()).or_insert(0) += 1;
        }
        let mut languages: Vec<(&str, usize)> = languages.into_iter().collect();
        languages.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        for (lang, count) in languages {
            out.push_str(&format!("| {lang} | {count} |\n"));
        }

        out.push_str("\n## Modules\n\n| Module | Files | Symbols |\n| --- | --- | --- |\n");
        for module in modules.iter().take(MAX_WIKI_MODULES) {
            out.push_str(&format!(
                "| [{}](modules/{}.md) | {} | {} |\n",
                module.name,
                slug(&module.name),
                module.files,
                module.symbols
            ));
        }
        if modules.len() > MAX_WIKI_MODULES {
            out.push_str(&format!(
                "\n_{} more modules not expanded._\n",
                modules.len() - MAX_WIKI_MODULES
            ));
        }

        out.push_str("\n## Entry points\n\n");
        let mut entries = 0;
        for entry in ENTRY_FILES {
            if let Some(file) = self.outline(entry) {
                entries += 1;
                let names: Vec<&str> = file
                    .symbols
                    .iter()
                    .take(6)
                    .map(|symbol| symbol.name.as_str())
                    .collect();
                out.push_str(&format!(
                    "- `{}` — {}\n",
                    file.path,
                    if names.is_empty() {
                        "no symbols".to_string()
                    } else {
                        names.join(", ")
                    }
                ));
            }
        }
        if entries == 0 {
            out.push_str("_No conventional entry file found._\n");
        }

        out.push_str(
            "\n## Lookups\n\nUse `find_symbols` for definitions, `search_text` for usages, and \
             `read_file` with the listed line number to see the body.\n",
        );
        out
    }

    fn render_module(&self, module: &str) -> String {
        let mut out = format!("# {module}\n\n");
        let files: Vec<&FileEntry> = self
            .files
            .iter()
            .filter(|file| module_of(&file.path) == module)
            .collect();
        let symbols: usize = files.iter().map(|file| file.symbols.len()).sum();
        out.push_str(&format!("{} files, {} symbols.\n\n", files.len(), symbols));
        for file in files.iter().take(MAX_SYMBOLS_PER_FILE) {
            out.push_str(&format!("## `{}` ({})\n\n", file.path, file.lang));
            if file.symbols.is_empty() {
                out.push_str("- _no symbols found_\n");
            }
            for symbol in file.symbols.iter().take(40) {
                let container = symbol
                    .container
                    .as_deref()
                    .map(|c| format!(" in `{c}`"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {} `{}`{}:{} — `{}`\n",
                    symbol.kind, symbol.name, container, symbol.line, symbol.signature
                ));
            }
            out.push('\n');
        }
        if files.len() > MAX_SYMBOLS_PER_FILE {
            out.push_str(&format!(
                "_{} more files not listed._\n",
                files.len() - MAX_SYMBOLS_PER_FILE
            ));
        }
        out
    }
}

/// Compact repo map injected into the system prompt; `None` when no index was ever built.
///
/// Reads the small `summary.json`, never the full index — this runs on every prompt build.
pub fn repo_map(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(CodeIndex::summary_path_for(root)).ok()?;
    let summary: IndexSummary = serde_json::from_str(&raw).ok()?;
    if summary.files == 0 {
        return None;
    }
    let mut out = String::new();
    out.push_str(&format!(
        "{} files / {} symbols indexed",
        summary.files, summary.symbols
    ));
    if let Some(updated) = summary.updated_at {
        out.push_str(&format!(" (updated {})", updated.format("%Y-%m-%d %H:%M")));
    }
    out.push('\n');
    for module in summary.modules.iter().take(12) {
        out.push_str(&format!(
            "- `{}` — {} files, {} symbols\n",
            module.name, module.files, module.symbols
        ));
    }
    if summary.modules.len() > 12 {
        out.push_str(&format!(
            "- _and {} more folders_\n",
            summary.modules.len() - 12
        ));
    }
    out.push_str(
        "Run `index_repo` to refresh after large changes, `find_symbols` to locate a definition, \
         `read_file` to read it.",
    );
    Some(out)
}

fn write_file(path: &Path, body: &str) -> Result<()> {
    std::fs::write(path, body).with_context(|| format!("write {}", path.display()))
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name) || name.starts_with(".synbot")
}

fn module_of(path: &str) -> &str {
    path.split_once('/')
        .map(|(head, _)| head)
        .unwrap_or("(root)")
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|delta| delta.as_millis() as u64)
        .unwrap_or(0)
}

fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// `Type::method` / `Type.method` → `(Some("Type"), "method")`; otherwise `(None, query)`.
fn split_query(query: &str) -> (Option<&str>, String) {
    let query = query.trim().trim_end_matches('(');
    for separator in ["::", "."] {
        if let Some((scope, name)) = query.rsplit_once(separator) {
            if !scope.is_empty() && !name.is_empty() {
                return (Some(scope.trim()), name.trim().to_string());
            }
        }
    }
    (None, query.to_string())
}

fn score_symbol(symbol: &Symbol, name: &str, scope: Option<&str>) -> Option<u32> {
    if let Some(scope) = scope {
        let container = symbol.container.as_deref().unwrap_or("");
        if !container.eq_ignore_ascii_case(scope) && !container.contains(scope) {
            return None;
        }
    }
    let exact = symbol.name == name;
    let lower = symbol.name.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut score = if exact {
        1000
    } else if lower == needle {
        900
    } else if lower.starts_with(&needle) {
        700
    } else if lower.contains(&needle) {
        450
    } else {
        return None;
    };
    // Callables are what people usually look for; bump them over same-named types.
    if matches!(
        symbol.kind.as_str(),
        "fn" | "method" | "function" | "def" | "func"
    ) {
        score += 20;
    }
    if scope.is_some() {
        score += 10;
    }
    Some(score)
}

// ---- Symbol extraction ----

/// A parsed definition, before the container stack resolves its parent.
struct Parsed {
    kind: &'static str,
    name: String,
    /// Whether following, more indented lines belong to this definition.
    opens_container: bool,
    /// Explicit container that overrides the indentation stack (Go receiver, `impl` target).
    container: Option<String>,
}

fn extract_symbols(lang: Lang, content: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut in_block_comment = false;

    for (index, raw) in content.lines().enumerate() {
        let trimmed = raw.trim();
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("/*") {
            in_block_comment = !trimmed.contains("*/");
            continue;
        }
        if is_comment_line(trimmed, lang) {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        // A line at the same or lower indentation closes the containers above it.
        while matches!(stack.last(), Some((level, _)) if *level >= indent) {
            stack.pop();
        }
        let Some(parsed) = parse_line(lang, trimmed) else {
            continue;
        };
        let container = parsed
            .container
            .clone()
            .or_else(|| stack.last().map(|(_, name)| name.clone()));
        out.push(Symbol {
            name: parsed.name.clone(),
            kind: parsed.kind.to_string(),
            line: index + 1,
            column: indent + 1,
            container,
            signature: signature_of(trimmed),
        });
        if parsed.opens_container {
            stack.push((indent, parsed.name));
        }
        if out.len() >= MAX_SYMBOLS_PER_FILE {
            break;
        }
    }
    out
}

fn is_comment_line(trimmed: &str, lang: Lang) -> bool {
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("<!--") {
        return true;
    }
    // Rust attributes and Python/Java decorators never define anything themselves.
    if trimmed.starts_with("#[") || trimmed.starts_with("#![") || trimmed.starts_with('@') {
        return true;
    }
    if matches!(lang, Lang::Python | Lang::Ruby | Lang::Php | Lang::CSharp)
        && trimmed.starts_with('#')
    {
        return true;
    }
    false
}

fn signature_of(line: &str) -> String {
    let head = line.split('{').next().unwrap_or(line).trim_end();
    let head = head.trim_end_matches(';').trim_end();
    let mut out: String = head.chars().take(160).collect();
    if head.chars().count() > 160 {
        out.push('…');
    }
    out
}

fn parse_line(lang: Lang, line: &str) -> Option<Parsed> {
    match lang {
        Lang::Rust => parse_rust(line),
        Lang::Python => parse_python(line),
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript | Lang::Jsx => parse_js(line),
        Lang::Go => parse_go(line),
        Lang::Ruby => parse_ruby(line),
        Lang::Kotlin => parse_kotlin(line),
        Lang::Swift => parse_swift(line),
        Lang::Php => parse_php(line),
        Lang::Java | Lang::CSharp | Lang::C | Lang::Cpp => parse_c_like(line),
    }
}

fn parsed(kind: &'static str, name: String, opens_container: bool) -> Option<Parsed> {
    if name.is_empty() {
        None
    } else {
        Some(Parsed {
            kind,
            name,
            opens_container,
            container: None,
        })
    }
}

/// First identifier of `text` (leading `&`, `*`, `'a ` and `mut ` are skipped).
fn ident(text: &str) -> String {
    let text = text.trim_start();
    let text = text.trim_start_matches(['&', '*', '~']).trim_start();
    let text = text.strip_prefix("mut ").unwrap_or(text).trim_start();
    text.chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
        .collect()
}

/// Last type-ish identifier of `text`, generics and paths stripped (`a::b::Vec<T>` → `Vec`).
fn type_name(text: &str) -> String {
    let text = text.trim();
    let text = text.split('<').next().unwrap_or(text);
    let text = text.trim_start_matches(['&', '*']).trim();
    let text = text.strip_prefix("mut ").unwrap_or(text).trim();
    let last = text.rsplit("::").next().unwrap_or(text);
    ident(last)
}

/// Last whitespace-separated token, then [`type_name`]: Go receivers look like `h *Hub`.
fn last_type_token(text: &str) -> String {
    text.split_whitespace()
        .last()
        .map(|token| type_name(token.trim_start_matches('*')))
        .unwrap_or_default()
}

fn strip_prefixes<'a>(mut text: &'a str, prefixes: &[&str]) -> &'a str {
    loop {
        let before = text;
        for prefix in prefixes {
            if let Some(rest) = text.strip_prefix(prefix) {
                text = rest;
                break;
            }
        }
        if text == before {
            return text;
        }
    }
}

fn parse_rust(line: &str) -> Option<Parsed> {
    let mut text = line;
    loop {
        let before = text;
        text = strip_prefixes(
            text,
            &[
                "pub(crate) ",
                "pub(super) ",
                "pub(self) ",
                "pub ",
                "async ",
                "unsafe ",
                "default ",
                "extern \"C\" ",
                "extern \"Rust\" ",
                "extern \"system\" ",
            ],
        );
        // `const fn` is a function; `const NAME` is a constant.
        if let Some(rest) = text.strip_prefix("const ") {
            if rest.starts_with("fn ") || rest.starts_with("async ") || rest.starts_with("unsafe ")
            {
                text = rest;
            }
        }
        if text == before {
            break;
        }
    }

    if let Some(rest) = text.strip_prefix("impl") {
        if rest.is_empty() || rest.starts_with(['<', ' ', '{']) {
            let target = rust_impl_target(rest);
            if !target.is_empty() {
                return Some(Parsed {
                    kind: "impl",
                    name: target.clone(),
                    opens_container: true,
                    container: None,
                });
            }
        }
    }
    for (prefix, kind, opens) in [
        ("fn ", "fn", false),
        ("struct ", "struct", true),
        ("enum ", "enum", true),
        ("trait ", "trait", true),
        ("union ", "union", true),
        ("mod ", "mod", true),
        ("type ", "type", false),
        ("static ", "static", false),
        ("const ", "const", false),
        ("macro_rules! ", "macro", false),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }
    None
}

/// `impl<T> Trait for Type` → `Type`.
fn rust_impl_target(rest: &str) -> String {
    let mut text = rest.trim_start();
    // Skip the generic parameter list, which may contain `for` itself (`impl<T: Fn()> …`).
    if text.starts_with('<') {
        let mut depth = 0usize;
        for (index, ch) in text.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        text = text[index + 1..].trim_start();
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let head = text.split('{').next().unwrap_or(text).trim();
    let head = head.trim_end_matches("where").trim();
    let target = head.rsplit_once(" for ").map(|(_, t)| t).unwrap_or(head);
    type_name(target)
}

fn parse_python(line: &str) -> Option<Parsed> {
    let text = line.strip_prefix("async ").unwrap_or(line);
    if let Some(rest) = text.strip_prefix("def ") {
        return parsed("def", ident(rest), false);
    }
    if let Some(rest) = text.strip_prefix("class ") {
        return parsed("class", ident(rest), true);
    }
    None
}

fn parse_js(line: &str) -> Option<Parsed> {
    let text = strip_prefixes(
        line,
        &[
            "export default ",
            "export ",
            "declare ",
            "abstract ",
            "async ",
            "static ",
            "public ",
            "private ",
            "protected ",
            "readonly ",
            "override ",
        ],
    );

    for (prefix, kind, opens) in [
        ("function ", "function", false),
        ("function* ", "function", false),
        ("class ", "class", true),
        ("interface ", "interface", true),
        ("enum ", "enum", true),
        ("namespace ", "namespace", true),
        ("type ", "type", false),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }

    if let Some(rest) = text.strip_prefix("const ") {
        let name = ident(rest);
        let after = rest[name.len()..].trim_start();
        let rhs = after.strip_prefix('=')?.trim_start();
        let callable = rhs.starts_with("async")
            || rhs.starts_with("function")
            || rhs.contains("=>")
            || rhs.starts_with('(');
        return parsed(if callable { "function" } else { "const" }, name, false);
    }

    // Method shorthand inside a class/object: `name(args) {`, `get name() {`, `#name(args) {`.
    if let Some(open) = text.find('(') {
        let head = text[..open].trim();
        let head = head
            .strip_prefix("get ")
            .or_else(|| head.strip_prefix("set "))
            .unwrap_or(head);
        if !text.ends_with('{') || head.contains('=') || head.contains(' ') {
            return None;
        }
        const NOT_METHODS: &[&str] = &[
            "if", "for", "while", "switch", "catch", "return", "function", "typeof", "new", "do",
            "else", "try", "with", "await", "yield", "super",
        ];
        if NOT_METHODS.contains(&head) {
            return None;
        }
        let name = ident(head);
        if name.len() == head.len() || head.starts_with('#') {
            return parsed("method", name, false);
        }
    }
    None
}

fn parse_go(line: &str) -> Option<Parsed> {
    if let Some(rest) = line.strip_prefix("func ") {
        let rest = rest.trim_start();
        if let Some(after_paren) = rest.strip_prefix('(') {
            let close = after_paren.find(')')?;
            let receiver = last_type_token(&after_paren[..close]);
            let name = ident(&after_paren[close + 1..]);
            if name.is_empty() {
                return None;
            }
            return Some(Parsed {
                kind: "method",
                name,
                opens_container: false,
                container: Some(receiver),
            });
        }
        return parsed("func", ident(rest), false);
    }
    if let Some(rest) = line.strip_prefix("type ") {
        let name = ident(rest);
        let after = rest[name.len()..].trim_start();
        let kind = if after.starts_with("struct") {
            "struct"
        } else if after.starts_with("interface") {
            "interface"
        } else {
            "type"
        };
        return parsed(kind, name, true);
    }
    None
}

fn parse_kotlin(line: &str) -> Option<Parsed> {
    let text = strip_prefixes(
        line,
        &[
            "public ",
            "private ",
            "protected ",
            "internal ",
            "open ",
            "abstract ",
            "data ",
            "sealed ",
            "override ",
            "suspend ",
            "inline ",
        ],
    );
    if let Some(rest) = text.strip_prefix("fun ") {
        // `fun <T> name(` — skip the generic parameter list.
        let rest = if rest.starts_with('<') {
            rest.split_once('>')
                .map(|(_, tail)| tail.trim_start())
                .unwrap_or(rest)
        } else {
            rest
        };
        let name = rest
            .split_once('(')
            .map(|(name, _)| type_name(name))
            .or_else(|| Some(ident(rest)))
            .unwrap_or_default();
        return parsed("fun", name, false);
    }
    for (prefix, kind, opens) in [
        ("class ", "class", true),
        ("interface ", "interface", true),
        ("object ", "object", true),
        ("enum class ", "enum", true),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }
    None
}

fn parse_swift(line: &str) -> Option<Parsed> {
    let text = strip_prefixes(
        line,
        &[
            "public ",
            "private ",
            "internal ",
            "fileprivate ",
            "open ",
            "final ",
            "mutating ",
            "static ",
            "override ",
            "class ",
        ],
    );
    for (prefix, kind, opens) in [
        ("func ", "func", false),
        ("struct ", "struct", true),
        ("protocol ", "protocol", true),
        ("extension ", "extension", true),
        ("enum ", "enum", true),
        ("actor ", "class", true),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }
    // `class Foo` was swallowed by the modifier stripper above.
    if line.contains("class ") {
        if let Some(rest) = line.split_once("class ").map(|(_, tail)| tail) {
            return parsed("class", ident(rest), true);
        }
    }
    None
}

fn parse_ruby(line: &str) -> Option<Parsed> {
    if let Some(rest) = line.strip_prefix("def ") {
        let name = rest.split(['(', ' ']).next().map(ident).unwrap_or_default();
        return parsed("def", name, false);
    }
    for (prefix, kind, opens) in [("class ", "class", true), ("module ", "module", true)] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }
    None
}

fn parse_php(line: &str) -> Option<Parsed> {
    let text = strip_prefixes(
        line,
        &[
            "public ",
            "private ",
            "protected ",
            "static ",
            "abstract ",
            "final ",
            "async ",
        ],
    );
    if let Some(rest) = text.strip_prefix("function ") {
        return parsed("function", ident(rest), false);
    }
    for (prefix, kind, opens) in [
        ("class ", "class", true),
        ("interface ", "interface", true),
        ("trait ", "trait", true),
        ("enum ", "enum", true),
        ("namespace ", "namespace", true),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }
    None
}

fn parse_c_like(line: &str) -> Option<Parsed> {
    let text = strip_prefixes(
        line,
        &[
            "public ",
            "private ",
            "protected ",
            "internal ",
            "static ",
            "final ",
            "abstract ",
            "synchronized ",
            "native ",
            "transient ",
            "volatile ",
            "sealed ",
            "partial ",
            "virtual ",
            "inline ",
            "explicit ",
            "constexpr ",
            "template ",
            "export ",
            "async ",
            "readonly ",
            "override ",
            "unsafe ",
        ],
    );
    for (prefix, kind, opens) in [
        ("class ", "class", true),
        ("interface ", "interface", true),
        ("struct ", "struct", true),
        ("union ", "union", true),
        ("enum ", "enum", true),
        ("namespace ", "namespace", true),
        ("record ", "record", true),
        ("typedef ", "type", false),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parsed(kind, ident(rest), opens);
        }
    }

    // Function or method: `Type name(args…)` with the body opening on this line.
    if !text.ends_with('{') || !text.contains('(') {
        return None;
    }
    let head = text[..text.find('(')?].trim();
    if head.is_empty() || head.contains('=') || head.contains("operator") {
        return None;
    }
    const NOT_FUNCTIONS: &[&str] = &[
        "if", "for", "while", "switch", "catch", "return", "sizeof", "do", "else", "try", "new",
        "delete", "throw", "case", "using", "foreach", "lock", "when",
    ];
    let name = ident(head.rsplit([' ', '*', '&', ':']).next().unwrap_or(head));
    if name.is_empty() || NOT_FUNCTIONS.contains(&name.as_str()) {
        return None;
    }
    // A definition has a return type (or a constructor/destructor name) before the arguments.
    parsed("method", name, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, body: &str) {
        let full = root.join(path);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(full, body).unwrap();
    }

    #[test]
    fn extracts_rust_items_including_impl_methods() {
        let source = r#"
use std::path::Path;

pub const LIMIT: usize = 4;

#[derive(Debug)]
pub struct Session {
    id: String,
}

impl Session {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    async fn run(&self) {}
}

pub trait Store {
    fn save(&self);
}

mod inner {
    fn helper() {}
}
"#;
        let symbols = extract_symbols(Lang::Rust, source);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"LIMIT"), "{names:?}");
        assert!(names.contains(&"Session"), "{names:?}");
        assert!(names.contains(&"new"), "{names:?}");
        assert!(names.contains(&"run"), "{names:?}");
        assert!(names.contains(&"helper"), "{names:?}");

        let new = symbols.iter().find(|s| s.name == "new").unwrap();
        assert_eq!(new.kind, "fn");
        assert_eq!(new.container.as_deref(), Some("Session"));
        assert_eq!(new.line, 12);
        let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
        assert_eq!(helper.container.as_deref(), Some("inner"));
    }

    #[test]
    fn extracts_typescript_python_and_go() {
        let ts = r#"
export interface User { id: string }

export const useUser = (id: string) => id;

export class Repo {
  async load(path: string): Promise<void> {
    return;
  }
}

function helper() {}
"#;
        let symbols = extract_symbols(Lang::TypeScript, ts);
        let kinds: Vec<(&str, &str)> = symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert!(kinds.contains(&("User", "interface")), "{kinds:?}");
        assert!(kinds.contains(&("useUser", "function")), "{kinds:?}");
        assert!(kinds.contains(&("Repo", "class")), "{kinds:?}");
        assert!(kinds.contains(&("load", "method")), "{kinds:?}");
        let load = symbols.iter().find(|s| s.name == "load").unwrap();
        assert_eq!(load.container.as_deref(), Some("Repo"));

        let py = "class Worker:\n    def run(self):\n        pass\n\ndef main():\n    pass\n";
        let symbols = extract_symbols(Lang::Python, py);
        let run = symbols.iter().find(|s| s.name == "run").unwrap();
        assert_eq!(run.container.as_deref(), Some("Worker"));
        assert_eq!(run.column, 5);
        let main = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(main.container, None);

        let go =
            "package x\n\nfunc (h *Hub) Open() {}\n\nfunc Build() {}\n\ntype Hub struct {\n}\n";
        let symbols = extract_symbols(Lang::Go, go);
        let open = symbols.iter().find(|s| s.name == "Open").unwrap();
        assert_eq!(open.kind, "method");
        assert_eq!(open.container.as_deref(), Some("Hub"));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Build" && s.kind == "func"));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Hub" && s.kind == "struct"));
    }

    #[test]
    fn ignores_comments_attributes_and_block_comments() {
        let rust = "// fn commented() {}\n/* fn hidden() {} */\n#[test]\nfn real() {}\n";
        let names: Vec<String> = extract_symbols(Lang::Rust, rust)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn refresh_is_incremental_and_drops_deleted_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/a.rs", "pub fn alpha() {}\n");
        write(dir.path(), "src/b.rs", "pub struct Beta;\n");
        write(
            dir.path(),
            "node_modules/x/index.js",
            "export function ignored() {}\n",
        );
        write(dir.path(), "README.md", "# hi\n");

        let mut index = CodeIndex::empty(dir.path());
        let first = index.refresh(dir.path()).unwrap();
        assert_eq!(first.parsed, 2, "node_modules and markdown are skipped");
        assert_eq!(first.files, 2);
        assert_eq!(first.symbols, 2);
        assert_eq!(first.languages.get("rust"), Some(&2));

        let second = index.refresh(dir.path()).unwrap();
        assert_eq!(second.parsed, 0);
        assert_eq!(second.reused, 2);
        assert_eq!(second.removed, 0);

        // Touching one file re-parses only that file.
        write(
            dir.path(),
            "src/a.rs",
            "pub fn alpha() {}\npub fn gamma() {}\n",
        );
        let third = index.refresh(dir.path()).unwrap();
        assert_eq!(third.parsed, 1);
        assert_eq!(third.reused, 1);
        assert_eq!(third.symbols, 3);

        std::fs::remove_file(dir.path().join("src/b.rs")).unwrap();
        let fourth = index.refresh(dir.path()).unwrap();
        assert_eq!(fourth.removed, 1);
        assert_eq!(fourth.files, 1);
        assert!(CodeIndex::path_for(dir.path()).exists());
        assert!(CodeIndex::summary_path_for(dir.path()).exists());
    }

    #[test]
    fn search_ranks_exact_matches_and_honours_scope() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/a.rs",
            "pub fn run() {}\npub struct Runner;\nimpl Runner {\n    fn start(&self) {}\n}\n",
        );
        write(dir.path(), "src/b.rs", "fn run_fast() {}\n");
        let mut index = CodeIndex::empty(dir.path());
        index.refresh(dir.path()).unwrap();

        let hits = index.search("run", 10);
        assert_eq!(hits[0].name, "run", "exact match first: {hits:?}");
        assert_eq!(hits[0].path, "src/a.rs");

        let scoped = index.search("Runner::start", 10);
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].name, "start");
        assert_eq!(scoped[0].container.as_deref(), Some("Runner"));

        assert!(index.search("nothing_here", 10).is_empty());
        // `limit` is clamped, never zero results from a positive request.
        assert_eq!(index.search("run", 1).len(), 1);
    }

    #[test]
    fn skips_oversized_files_and_survives_a_corrupt_index() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "big.rs", &"pub fn f() {}\n".repeat(60_000));
        let mut index = CodeIndex::empty(dir.path());
        let stats = index.refresh(dir.path()).unwrap();
        assert_eq!(stats.files, 0, ">512KB files are skipped");

        std::fs::write(CodeIndex::path_for(dir.path()), "{not json").unwrap();
        let loaded = CodeIndex::load(dir.path());
        assert!(loaded.files.is_empty());
    }

    #[test]
    fn writes_wiki_pages_and_exposes_a_repo_map() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/lib.rs",
            "pub fn api() {}\npub struct Client;\n",
        );
        write(dir.path(), "web/index.ts", "export function mount() {}\n");
        let mut index = CodeIndex::empty(dir.path());
        index.refresh(dir.path()).unwrap();

        let report = index.write_wiki(dir.path()).unwrap();
        assert!(report.files.iter().any(|f| f == "index.md"));
        assert_eq!(report.modules, 2);
        let index_md = std::fs::read_to_string(dir.path().join(WIKI_DIR).join("index.md")).unwrap();
        assert!(index_md.contains("# Repository index"));
        assert!(index_md.contains("[src](modules/src.md)"));
        assert!(index_md.contains("`src/lib.rs`"), "{index_md}");
        let module_md =
            std::fs::read_to_string(dir.path().join(WIKI_DIR).join("modules/src.md")).unwrap();
        assert!(module_md.contains("`Client`"), "{module_md}");

        let map = repo_map(dir.path()).expect("summary written by refresh");
        assert!(map.contains("2 files / 3 symbols"), "{map}");
        assert!(map.contains("`src` — 1 files, 2 symbols"), "{map}");
        assert!(repo_map(&dir.path().join("elsewhere")).is_none());
    }
}
