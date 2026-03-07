//! Code analysis tools: project scanning, symbol extraction, and context search.
//!
//! Provides `CodeAnalyzerTool` (registered as `analyze_code`) which supports
//! two actions: `scan_project` and `search_context`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::tools::context;
use crate::tools::DynTool;

/// Mapping of well-known project marker files to their project type identifiers.
pub const PROJECT_TYPE_MARKERS: &[(&str, &str)] = &[
    ("Cargo.toml", "rust/cargo"),
    ("package.json", "node/npm"),
    ("pyproject.toml", "python/pip"),
    ("setup.py", "python/setuptools"),
    ("go.mod", "go/mod"),
    ("pom.xml", "java/maven"),
    ("build.gradle", "java/gradle"),
    ("CMakeLists.txt", "cpp/cmake"),
    ("Makefile", "generic/make"),
];

/// Result of scanning a project workspace.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProjectScanResult {
    /// Project type identifier (e.g. "rust/cargo", "node/npm").
    pub project_type: String,
    /// Path to the build configuration file, if detected.
    pub build_config: Option<String>,
    /// Directory → file list mapping.
    pub file_tree: BTreeMap<String, Vec<String>>,
    /// Top-level symbols extracted from source files.
    pub symbols: Vec<SymbolInfo>,
    /// Scan statistics.
    pub stats: ScanStats,
}

/// A top-level symbol found in a source file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    /// File path (relative to workspace).
    pub file: String,
    /// Symbol name.
    pub name: String,
    /// Symbol kind ("mod", "fn", "struct", "trait", "impl", "class", "def", etc.).
    pub kind: String,
    /// Line number (1-based).
    pub line: usize,
}

/// Statistics about a project scan.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScanStats {
    /// Total number of files found.
    pub total_files: usize,
    /// Number of files actually scanned for symbols.
    pub scanned_files: usize,
    /// Number of files skipped because they exceeded the size limit.
    pub skipped_large_files: usize,
    /// Whether the result was truncated due to file count limits.
    pub truncated: bool,
}

/// Collected code context for a search query.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CodeContext {
    /// Matching code snippets.
    pub snippets: Vec<CodeSnippet>,
    /// Symbols from referenced/imported modules.
    pub referenced_symbols: Vec<SymbolInfo>,
    /// Whether the context was truncated due to token limits.
    pub truncated: bool,
}

/// A code snippet extracted from a source file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CodeSnippet {
    /// File path (relative to workspace).
    pub file: String,
    /// Start line number (1-based).
    pub start_line: usize,
    /// End line number (1-based, inclusive).
    pub end_line: usize,
    /// The code content.
    pub content: String,
    /// Relevance score (0.0–1.0).
    pub relevance: f32,
}

/// Detect the project type by checking for well-known marker files in the workspace root.
///
/// Returns `(project_type, build_config_path)`. If no marker is found, returns
/// `("unknown", None)`.
pub fn detect_project_type(workspace: &Path) -> (String, Option<String>) {
    for &(marker, project_type) in PROJECT_TYPE_MARKERS {
        if workspace.join(marker).exists() {
            return (project_type.to_string(), Some(marker.to_string()));
        }
    }
    ("unknown".to_string(), None)
}

/// Directories to skip during recursive scanning.
const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    ".git",
    ".hg",
    ".svn",
    "dist",
    "build",
    ".idea",
    ".vscode",
    "vendor",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".next",
    ".nuxt",
];

/// Source file extensions eligible for symbol extraction.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "go", "java", "cpp", "c", "h",
];

/// File type priority for truncation (lower index = higher priority).
/// Source code files are kept over non-source files.
const PRIORITY_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "go", "java", "cpp", "c", "h",
    "toml", "json", "yaml", "yml", "xml", "md", "txt",
];

/// Returns the priority of a file extension (lower = higher priority).
/// Unknown extensions get the lowest priority.
fn extension_priority(path: &str) -> usize {
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        PRIORITY_EXTENSIONS
            .iter()
            .position(|&e| e == ext)
            .unwrap_or(PRIORITY_EXTENSIONS.len())
    } else {
        PRIORITY_EXTENSIONS.len() + 1
    }
}

/// Check if a file extension is a source code file eligible for symbol extraction.
fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

/// Recursively collect file paths from a directory, respecting ignore rules.
///
/// Populates `files` with `(relative_path, file_size)` tuples.
/// Increments `skipped_large` for files exceeding `max_file_size`.
fn collect_files(
    dir: &Path,
    workspace: &Path,
    max_file_size: u64,
    files: &mut Vec<(String, u64)>,
    skipped_large: &mut usize,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return, // skip unreadable directories
    };

    let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries_vec.sort_by_key(|e| e.file_name());

    for entry in entries_vec {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                // Skip hidden directories and common ignore dirs
                if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                collect_files(&path, workspace, max_file_size, files, skipped_large);
            } else if ft.is_file() {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let rel = path
                    .strip_prefix(workspace)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();

                if size > max_file_size {
                    *skipped_large += 1;
                } else {
                    files.push((rel, size));
                }
            }
        }
    }
}

/// Extract top-level symbols from a source file using simple string matching.
///
/// Returns a list of `SymbolInfo` for the given file.
fn extract_symbols(file_path: &str, content: &str) -> Vec<SymbolInfo> {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let mut symbols = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || (trimmed.starts_with('#') && ext != "c" && ext != "cpp" && ext != "h") {
            continue;
        }

        let found = match ext {
            "rs" => extract_rust_symbol(trimmed),
            "py" => extract_python_symbol(trimmed),
            "js" | "ts" => extract_js_ts_symbol(trimmed),
            "go" => extract_go_symbol(trimmed),
            "java" => extract_java_symbol(trimmed),
            "c" | "cpp" | "h" => extract_c_cpp_symbol(trimmed),
            _ => None,
        };

        if let Some((kind, name)) = found {
            symbols.push(SymbolInfo {
                file: file_path.to_string(),
                name,
                kind: kind.to_string(),
                line: line_idx + 1, // 1-based
            });
        }
    }

    symbols
}

/// Extract a Rust top-level symbol from a trimmed line.
fn extract_rust_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    // Order matters: check longer prefixes first
    let patterns: &[(&str, &str)] = &[
        ("pub mod ", "mod"),
        ("mod ", "mod"),
        ("pub fn ", "fn"),
        ("fn ", "fn"),
        ("pub struct ", "struct"),
        ("struct ", "struct"),
        ("pub trait ", "trait"),
        ("trait ", "trait"),
        ("pub enum ", "enum"),
        ("enum ", "enum"),
        ("impl ", "impl"),
    ];

    for &(prefix, kind) in patterns {
        if trimmed.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            let name = extract_identifier(rest);
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

/// Extract a Python top-level symbol from a trimmed line.
fn extract_python_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    if trimmed.starts_with("def ") {
        let rest = &trimmed[4..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("def", name));
        }
    } else if trimmed.starts_with("class ") {
        let rest = &trimmed[6..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("class", name));
        }
    }
    None
}

/// Extract a JS/TS top-level symbol from a trimmed line.
fn extract_js_ts_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    let patterns: &[(&str, &str)] = &[
        ("export function ", "function"),
        ("export class ", "class"),
        ("export default function ", "function"),
        ("export default class ", "class"),
        ("function ", "function"),
        ("class ", "class"),
    ];

    for &(prefix, kind) in patterns {
        if trimmed.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            let name = extract_identifier(rest);
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }

    // Handle `const NAME = ` or `export const NAME = `
    let rest = if trimmed.starts_with("export const ") {
        Some(&trimmed[13..])
    } else if trimmed.starts_with("const ") {
        Some(&trimmed[6..])
    } else {
        None
    };

    if let Some(rest) = rest {
        let name = extract_identifier(rest);
        if !name.is_empty() && rest[name.len()..].trim_start().starts_with('=') {
            return Some(("const", name));
        }
    }

    None
}

/// Extract a Go top-level symbol from a trimmed line.
fn extract_go_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    if trimmed.starts_with("func ") {
        let rest = &trimmed[5..];
        // Skip method receivers: func (r *Receiver) Name(...)
        let rest = if rest.starts_with('(') {
            // Find closing paren, then extract name after it
            if let Some(close) = rest.find(')') {
                rest[close + 1..].trim_start()
            } else {
                return None;
            }
        } else {
            rest
        };
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("func", name));
        }
    } else if trimmed.starts_with("type ") {
        let rest = &trimmed[5..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            let after_name = rest[name.len()..].trim_start();
            if after_name.starts_with("struct") {
                return Some(("struct", name));
            } else if after_name.starts_with("interface") {
                return Some(("interface", name));
            }
        }
    }
    None
}

/// Extract a Java top-level symbol from a trimmed line.
fn extract_java_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    let patterns: &[(&str, &str)] = &[
        ("public class ", "class"),
        ("public interface ", "interface"),
        ("public abstract class ", "class"),
        ("public enum ", "enum"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("abstract class ", "class"),
        ("enum ", "enum"),
    ];

    for &(prefix, kind) in patterns {
        if trimmed.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            let name = extract_identifier(rest);
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

/// Extract a C/C++ top-level symbol from a trimmed line.
fn extract_c_cpp_symbol(trimmed: &str) -> Option<(&'static str, String)> {
    if trimmed.starts_with("struct ") {
        let rest = &trimmed[7..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("struct", name));
        }
    } else if trimmed.starts_with("class ") {
        let rest = &trimmed[6..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("class", name));
        }
    } else if trimmed.starts_with("typedef ") {
        let rest = &trimmed[8..];
        let name = extract_identifier(rest);
        if !name.is_empty() {
            return Some(("typedef", name));
        }
    }
    None
}

/// Extract a valid identifier from the start of a string.
/// An identifier starts with a letter or underscore, followed by alphanumeric or underscore.
fn extract_identifier(s: &str) -> String {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => c,
        _ => return String::new(),
    };
    let mut name = String::new();
    name.push(first);
    for c in chars {
        if c.is_alphanumeric() || c == '_' {
            name.push(c);
        } else {
            break;
        }
    }
    name
}

/// Scan a workspace directory and return a structured project analysis.
///
/// - Recursively walks the directory tree, skipping hidden dirs and common ignore dirs.
/// - Skips files larger than `max_file_size` bytes (counted in `stats.skipped_large_files`).
/// - If total files exceed `max_scan_files`, truncates by file-type priority and sets `stats.truncated`.
/// - Extracts top-level symbols from source code files.
/// - Detects project type via `detect_project_type`.
///
/// Returns the `ProjectScanResult` serialized as a JSON string.
pub fn scan_project(
    workspace: &Path,
    max_file_size: u64,
    max_scan_files: usize,
) -> anyhow::Result<String> {
    let workspace = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());

    // 1. Collect all files recursively
    let mut all_files: Vec<(String, u64)> = Vec::new();
    let mut skipped_large_files: usize = 0;
    collect_files(&workspace, &workspace, max_file_size, &mut all_files, &mut skipped_large_files);

    let total_files = all_files.len() + skipped_large_files;

    // 2. Truncate if needed, prioritizing by file type
    let truncated = all_files.len() > max_scan_files;
    if truncated {
        // Sort by priority (source files first), then alphabetically
        all_files.sort_by(|a, b| {
            let pa = extension_priority(&a.0);
            let pb = extension_priority(&b.0);
            pa.cmp(&pb).then_with(|| a.0.cmp(&b.0))
        });
        all_files.truncate(max_scan_files);
    }

    // 3. Build file tree (directory → file list)
    let mut file_tree: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (rel_path, _size) in &all_files {
        let path = PathBuf::from(rel_path);
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());
        let dir_key = if dir.is_empty() { ".".to_string() } else { dir };
        let file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        file_tree.entry(dir_key).or_default().push(file_name);
    }

    // 4. Extract symbols from source files
    let mut symbols: Vec<SymbolInfo> = Vec::new();
    let mut scanned_files: usize = 0;
    for (rel_path, _size) in &all_files {
        let full_path = workspace.join(rel_path);
        if is_source_file(&full_path) {
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let file_symbols = extract_symbols(rel_path, &content);
                symbols.extend(file_symbols);
                scanned_files += 1;
            }
        }
    }

    // 5. Detect project type
    let (project_type, build_config) = detect_project_type(&workspace);

    // 6. Build result
    let result = ProjectScanResult {
        project_type,
        build_config,
        file_tree,
        symbols,
        stats: ScanStats {
            total_files,
            scanned_files,
            skipped_large_files,
            truncated,
        },
    };

    serde_json::to_string(&result).map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))
}

/// Intermediate match info before merging into snippets.
struct RawMatch {
    /// Matched line number (0-based).
    line_idx: usize,
    /// Relevance score for this match.
    relevance: f32,
}

/// Extract import/use references from file content, returning module names.
///
/// Supports:
/// - Rust: `use crate::module::...` or `mod module;`
/// - Python: `import module` or `from module import ...`
/// - JS/TS: `import ... from 'module'` or `require('module')`
fn extract_imports(content: &str, ext: &str) -> Vec<String> {
    let mut modules = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        match ext {
            "rs" => {
                // use crate::module_name::... or use crate::module_name;
                if trimmed.starts_with("use crate::") {
                    let rest = &trimmed["use crate::".len()..];
                    let module = extract_identifier(rest);
                    if !module.is_empty() {
                        modules.push(module);
                    }
                } else if trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ") {
                    let rest = if trimmed.starts_with("pub mod ") {
                        &trimmed["pub mod ".len()..]
                    } else {
                        &trimmed["mod ".len()..]
                    };
                    let module = extract_identifier(rest);
                    if !module.is_empty() && trimmed.ends_with(';') {
                        modules.push(module);
                    }
                }
            }
            "py" => {
                // import module or from module import ...
                if trimmed.starts_with("import ") {
                    let rest = &trimmed["import ".len()..];
                    let module = extract_identifier(rest);
                    if !module.is_empty() {
                        modules.push(module);
                    }
                } else if trimmed.starts_with("from ") {
                    let rest = &trimmed["from ".len()..];
                    let module = extract_identifier(rest);
                    if !module.is_empty() {
                        modules.push(module);
                    }
                }
            }
            "js" | "ts" => {
                // import ... from 'module' or require('module')
                if let Some(pos) = trimmed.find("from ") {
                    let after_from = &trimmed[pos + 5..];
                    let after_from = after_from.trim();
                    // Extract module name from quotes
                    if let Some(module) = extract_quoted_string(after_from) {
                        modules.push(module);
                    }
                } else if let Some(pos) = trimmed.find("require(") {
                    let after_req = &trimmed[pos + 8..];
                    if let Some(module) = extract_quoted_string(after_req) {
                        modules.push(module);
                    }
                }
            }
            _ => {}
        }
    }
    modules
}

/// Extract a string from inside quotes (single or double).
fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim();
    let (quote, rest) = if s.starts_with('\'') {
        ('\'', &s[1..])
    } else if s.starts_with('"') {
        ('"', &s[1..])
    } else {
        return None;
    };
    if let Some(end) = rest.find(quote) {
        let val = &rest[..end];
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

/// Try to resolve a module name to a file path within the workspace.
///
/// Checks common patterns: `module.rs`, `module/mod.rs`, `module.py`, `module.js`, `module.ts`.
fn resolve_module_path(workspace: &Path, module_name: &str) -> Option<PathBuf> {
    let candidates = [
        format!("{}.rs", module_name),
        format!("{}/mod.rs", module_name),
        format!("src/{}.rs", module_name),
        format!("src/{}/mod.rs", module_name),
        format!("{}.py", module_name),
        format!("{}.js", module_name),
        format!("{}.ts", module_name),
    ];
    for candidate in &candidates {
        let path = workspace.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Search the workspace for code relevant to a query and return structured context.
///
/// - Splits `query` into whitespace-separated, lowercased keywords.
/// - Recursively walks workspace files (skipping hidden/ignored dirs and large files).
/// - For each file, checks filename and content for keyword matches.
/// - Extracts code snippets with `context_lines` lines of surrounding context.
/// - Merges overlapping snippets from the same file.
/// - Tracks import/use statements and collects referenced module symbols.
/// - Sorts by relevance (descending), truncates when total exceeds `max_context_bytes`.
/// - Limits to `max_results` snippets.
///
/// Returns `CodeContext` serialized as JSON.
pub fn search_context(
    workspace: &Path,
    query: &str,
    max_results: usize,
    context_lines: usize,
    max_file_size: u64,
    max_context_bytes: usize,
) -> anyhow::Result<String> {
    let workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    // 1. Split query into lowercased keywords
    let keywords: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();

    if keywords.is_empty() {
        let ctx = CodeContext {
            snippets: vec![],
            referenced_symbols: vec![],
            truncated: false,
        };
        return serde_json::to_string(&ctx)
            .map_err(|e| anyhow::anyhow!("Failed to serialize: {}", e));
    }

    // 2. Collect all files
    let mut all_files: Vec<(String, u64)> = Vec::new();
    let mut skipped_large: usize = 0;
    collect_files(
        &workspace,
        &workspace,
        max_file_size,
        &mut all_files,
        &mut skipped_large,
    );

    // 3. Search each file for matches and build snippets
    let mut all_snippets: Vec<CodeSnippet> = Vec::new();
    let mut import_modules: Vec<String> = Vec::new();

    for (rel_path, _size) in &all_files {
        let full_path = workspace.join(rel_path);
        let file_name_lower = Path::new(rel_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Check filename match
        let filename_keyword_matches: usize = keywords
            .iter()
            .filter(|kw| file_name_lower.contains(kw.as_str()))
            .count();
        let filename_matches = filename_keyword_matches > 0;

        // Read file content
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Find content matches
        let mut raw_matches: Vec<RawMatch> = Vec::new();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_lower = line.to_lowercase();
            let kw_match_count = keywords
                .iter()
                .filter(|kw| line_lower.contains(kw.as_str()))
                .count();

            if kw_match_count > 0 {
                // Content match: base 0.5, boost for multiple keywords
                let relevance =
                    0.5 + 0.1 * (kw_match_count as f32 - 1.0).max(0.0);
                raw_matches.push(RawMatch {
                    line_idx,
                    relevance,
                });
            }
        }

        // If filename matches but no content matches, add a snippet for the file header
        if filename_matches && raw_matches.is_empty() {
            let end = context_lines.min(total_lines.saturating_sub(1));
            let snippet_lines = &lines[0..=end];
            let relevance =
                0.8 + 0.1 * (filename_keyword_matches as f32 - 1.0).max(0.0);
            all_snippets.push(CodeSnippet {
                file: rel_path.clone(),
                start_line: 1,
                end_line: end + 1,
                content: snippet_lines.join("\n"),
                relevance: relevance.min(1.0),
            });
        }

        if !raw_matches.is_empty() {
            // Boost relevance if filename also matches
            if filename_matches {
                for m in &mut raw_matches {
                    m.relevance = (m.relevance + 0.3).min(1.0);
                }
            }

            // Merge overlapping matches into snippet ranges
            // Sort by line index
            raw_matches.sort_by_key(|m| m.line_idx);

            let mut merged: Vec<(usize, usize, f32)> = Vec::new(); // (start, end, max_relevance) 0-based

            for m in &raw_matches {
                let start = m.line_idx.saturating_sub(context_lines);
                let end = (m.line_idx + context_lines).min(total_lines.saturating_sub(1));

                if let Some(last) = merged.last_mut() {
                    // Merge if overlapping or adjacent
                    if start <= last.1 + 1 {
                        last.1 = last.1.max(end);
                        last.2 = last.2.max(m.relevance);
                        continue;
                    }
                }
                merged.push((start, end, m.relevance));
            }

            // Build snippets from merged ranges
            for (start, end, relevance) in &merged {
                let snippet_lines = &lines[*start..=*end];
                all_snippets.push(CodeSnippet {
                    file: rel_path.clone(),
                    start_line: start + 1, // 1-based
                    end_line: end + 1,     // 1-based
                    content: snippet_lines.join("\n"),
                    relevance: *relevance,
                });
            }
        }

        // 4. Track imports from files that had any match
        if filename_matches || !raw_matches.is_empty() {
            let ext = Path::new(rel_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let imports = extract_imports(&content, ext);
            import_modules.extend(imports);
        }
    }

    // 5. Sort by relevance (descending), then by file path for stability
    all_snippets.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });

    // 6. Truncate by max_results
    let had_more_results = all_snippets.len() > max_results;
    all_snippets.truncate(max_results);

    // 7. Truncate by max_context_bytes
    let mut total_bytes: usize = 0;
    let mut truncated = had_more_results;
    let mut kept_snippets: Vec<CodeSnippet> = Vec::new();

    for snippet in all_snippets {
        let snippet_size = snippet.content.len();
        if total_bytes + snippet_size > max_context_bytes && !kept_snippets.is_empty() {
            truncated = true;
            break;
        }
        total_bytes += snippet_size;
        kept_snippets.push(snippet);
    }

    // 8. Resolve referenced symbols from imports
    let mut referenced_symbols: Vec<SymbolInfo> = Vec::new();
    // Deduplicate module names
    import_modules.sort();
    import_modules.dedup();

    for module_name in &import_modules {
        if let Some(module_path) = resolve_module_path(&workspace, module_name) {
            if let Ok(content) = std::fs::read_to_string(&module_path) {
                let rel = module_path
                    .strip_prefix(&workspace)
                    .unwrap_or(&module_path)
                    .to_string_lossy()
                    .into_owned();
                let symbols = extract_symbols(&rel, &content);
                referenced_symbols.extend(symbols);
            }
        }
    }

    // 9. Build and return CodeContext
    let ctx = CodeContext {
        snippets: kept_snippets,
        referenced_symbols,
        truncated,
    };

    serde_json::to_string(&ctx).map_err(|e| anyhow::anyhow!("Failed to serialize: {}", e))
}


// ---- CodeAnalyzerTool (DynTool) ----

/// Tool registered as `analyze_code` that dispatches to `scan_project` or `search_context`.
pub struct CodeAnalyzerTool {
    pub workspace: PathBuf,
    pub restrict: bool,
    pub max_file_size: u64,
    pub max_scan_files: usize,
}

#[async_trait::async_trait]
impl DynTool for CodeAnalyzerTool {
    fn name(&self) -> &str {
        "analyze_code"
    }

    fn description(&self) -> &str {
        "Analyze code structure, search context, and extract symbols in the workspace."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["scan_project", "search_context"],
                    "description": "Action: scan_project scans project structure; search_context searches relevant code"
                },
                "query": {
                    "type": "string",
                    "description": "Search query for search_context action (keywords, file patterns, symbol names)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 20)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines around matches (default: 5)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        // Determine effective workspace from tool context (agent scope) or fallback
        let effective_workspace = context::current_allowed_roots()
            .unwrap_or_else(|| self.workspace.clone());

        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'action' is required and must be a string"))?;

        match action {
            "scan_project" => {
                scan_project(&effective_workspace, self.max_file_size, self.max_scan_files)
            }
            "search_context" => {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'query' is required for search_context action"))?;
                let max_results = args["max_results"].as_u64().unwrap_or(20) as usize;
                let context_lines = args["context_lines"].as_u64().unwrap_or(5) as usize;

                search_context(
                    &effective_workspace,
                    query,
                    max_results,
                    context_lines,
                    self.max_file_size,
                    100_000,
                )
            }
            other => Err(anyhow::anyhow!("Unknown action: {}", other)),
        }
    }
}
