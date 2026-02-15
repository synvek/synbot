//! Memory system — daily notes + long-term MEMORY.md.
//!
//! **Not the same as sessions.** Sessions (in `~/.synbot/sessions/main/*.json` and
//! `~/.synbot/sessions/{role}/*.json`) store
//! raw conversation history (each message). Memory here is long-term and daily
//! notes used to build context for the model, stored under `~/.synbot/memory/{agentId}/`.
//!
//! Storage root is `~/.synbot/memory/{agentId}` (see `crate::config::memory_dir`).
//! Per agent: `MEMORY.md` (long-term), `memory/YYYY-MM-DD.md` (daily notes),
//! and optionally `{agentId}.sqlite` for index.

use chrono::{Local, NaiveDate};
use std::path::{Path, PathBuf};

use crate::config;

/// A parsed memory entry from a daily note file.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEntry {
    pub date: NaiveDate,
    pub content: String,
    pub tags: Vec<String>,
}

/// Root directory for one agent's memory: `~/.synbot/memory/{agentId}`.
pub struct MemoryStore {
    /// Agent memory root (e.g. ~/.synbot/memory/main).
    agent_root: PathBuf,
    /// Daily notes subdir: agent_root.join("memory").
    notes_dir: PathBuf,
}

/// Parse YAML front matter from a markdown file's content.
///
/// Front matter is delimited by `---` at the start and end.
/// Returns `(tags, content_after_front_matter)`.
///
/// Supports two tag formats:
/// - Inline: `tags: [tag1, tag2]`
/// - List:   `tags:\n- tag1\n- tag2`
fn parse_front_matter(raw: &str) -> (Vec<String>, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (Vec::new(), raw.to_string());
    }

    // Find the closing `---` (skip the opening one)
    let after_open = &trimmed[3..];
    let closing = after_open.find("\n---");
    let (front_matter_block, content) = match closing {
        Some(pos) => {
            let fm = &after_open[..pos];
            // Skip past the closing `---` and the newline after it
            let rest_start = pos + 4; // "\n---".len()
            let rest = &after_open[rest_start..];
            // Strip leading newline from content
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            (fm, rest.to_string())
        }
        None => {
            // No closing delimiter — treat entire content as regular content
            return (Vec::new(), raw.to_string());
        }
    };

    let tags = parse_tags_from_front_matter(front_matter_block);
    (tags, content)
}

/// Extract tags from a front matter block string.
///
/// Supports:
/// - `tags: [tag1, tag2, tag3]`
/// - `tags:\n- tag1\n- tag2`
fn parse_tags_from_front_matter(fm: &str) -> Vec<String> {
    let lines: Vec<&str> = fm.lines().collect();
    let mut tags = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("tags:") {
            let after_key = trimmed.strip_prefix("tags:").unwrap().trim();

            if after_key.starts_with('[') {
                // Inline format: tags: [tag1, tag2]
                let inner = after_key
                    .trim_start_matches('[')
                    .trim_end_matches(']');
                for tag in inner.split(',') {
                    let t = tag.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !t.is_empty() {
                        tags.push(t);
                    }
                }
            } else if after_key.is_empty() {
                // List format: tags:\n- tag1\n- tag2
                for subsequent in &lines[i + 1..] {
                    let sub = subsequent.trim();
                    if sub.starts_with("- ") {
                        let t = sub.strip_prefix("- ").unwrap().trim().to_string();
                        if !t.is_empty() {
                            tags.push(t);
                        }
                    } else if sub.starts_with('-') && sub.len() > 1 {
                        // Handle `- tag` without space after dash (unlikely but safe)
                        let t = sub[1..].trim().to_string();
                        if !t.is_empty() {
                            tags.push(t);
                        }
                    } else {
                        // End of list items
                        break;
                    }
                }
            }
            break;
        }
    }

    tags
}

/// Try to extract a date from a filename like `2025-01-15.md`.
fn date_from_filename(filename: &str) -> Option<NaiveDate> {
    let stem = filename.strip_suffix(".md")?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// Ensures memory dirs and MEMORY.md exist for main and all configured roles.
/// Call at startup so `~/.synbot/memory/{agentId}/` and `MEMORY.md` are created
/// even before any message is processed.
pub fn ensure_memory_dirs(cfg: &crate::config::Config) {
    let _ = MemoryStore::new("main");
    for role in &cfg.agent.roles {
        let _ = MemoryStore::new(&role.name);
    }
}

impl MemoryStore {
    /// Create a store for the given agent. Uses `~/.synbot/memory/{agent_id}`.
    /// Pass `"main"` or empty for the default agent.
    /// Ensures directories and an empty MEMORY.md exist so the structure is visible.
    pub fn new(agent_id: &str) -> Self {
        let agent_root = config::memory_dir(agent_id);
        std::fs::create_dir_all(&agent_root).ok();
        let notes_dir = agent_root.join("memory");
        std::fs::create_dir_all(&notes_dir).ok();
        // Ensure MEMORY.md exists so the memory dir is not empty and ready for appends
        let memory_file = agent_root.join("MEMORY.md");
        if !memory_file.exists() {
            let _ = std::fs::write(&memory_file, "# Long-term memory\n\n");
        }
        Self {
            agent_root,
            notes_dir,
        }
    }

    /// Create a store with an explicit agent root (e.g. for tests).
    /// Does not create MEMORY.md so tests can start with an empty store.
    pub fn new_with_root(agent_root: &Path) -> Self {
        std::fs::create_dir_all(agent_root).ok();
        let notes_dir = agent_root.join("memory");
        std::fs::create_dir_all(&notes_dir).ok();
        Self {
            agent_root: agent_root.to_path_buf(),
            notes_dir,
        }
    }

    /// Agent memory root directory (for tests and index path).
    pub fn agent_root(&self) -> &Path {
        &self.agent_root
    }

    /// Daily notes subdirectory (memory/YYYY-MM-DD.md). For tests.
    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    pub fn memory_file(&self) -> PathBuf {
        self.agent_root.join("MEMORY.md")
    }

    fn today_file(&self) -> PathBuf {
        self.notes_dir
            .join(format!("{}.md", Local::now().format("%Y-%m-%d")))
    }

    pub fn read_long_term(&self) -> String {
        std::fs::read_to_string(self.memory_file()).unwrap_or_default()
    }

    pub fn read_today(&self) -> String {
        std::fs::read_to_string(self.today_file()).unwrap_or_default()
    }

    pub fn get_recent_memories(&self, days: u32) -> String {
        let today = Local::now().date_naive();
        let mut parts = Vec::new();
        for i in 0..days {
            let date = today - chrono::Duration::days(i as i64);
            let path = self
                .notes_dir
                .join(format!("{}.md", date.format("%Y-%m-%d")));
            if let Ok(content) = std::fs::read_to_string(&path) {
                parts.push(content);
            }
        }
        parts.join("\n\n---\n\n")
    }

    /// Build the memory context section for the system prompt.
    pub fn get_memory_context(&self) -> String {
        let mut parts = Vec::new();
        let lt = self.read_long_term();
        if !lt.is_empty() {
            parts.push(format!("## Long-term Memory\n\n{}", lt));
        }
        let today = self.read_today();
        if !today.is_empty() {
            parts.push(format!("## Today's Notes\n\n{}", today));
        }
        parts.join("\n\n")
    }

    /// Build memory context with a configurable window of recent days.
    ///
    /// Like `get_memory_context` but includes recent memories from the
    /// last `days` days instead of only today's notes.
    pub fn get_memory_context_with_window(&self, days: u32) -> String {
        let mut parts = Vec::new();
        let lt = self.read_long_term();
        if !lt.is_empty() {
            parts.push(format!("## Long-term Memory\n\n{}", lt));
        }
        let recent = self.get_recent_memories(days);
        if !recent.is_empty() {
            parts.push(format!("## Recent Notes ({} days)\n\n{}", days, recent));
        }
        parts.join("\n\n")
    }

    /// Parse a single daily note file into a `MemoryEntry`.
    fn parse_memory_file(&self, path: &Path) -> Option<MemoryEntry> {
        let filename = path.file_name()?.to_str()?;
        let date = date_from_filename(filename)?;
        let raw = std::fs::read_to_string(path).ok()?;
        let (tags, content) = parse_front_matter(&raw);
        Some(MemoryEntry {
            date,
            content,
            tags,
        })
    }

    /// Search memory entries by date range and/or tags.
    ///
    /// - `from`: inclusive start date (if `None`, no lower bound)
    /// - `to`: inclusive end date (if `None`, no upper bound)
    /// - `tags`: if `Some`, only entries that contain **at least one** of the
    ///   specified tags are returned
    pub fn search(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        tags: Option<&[String]>,
    ) -> Vec<MemoryEntry> {
        let mut entries = Vec::new();

        let dir = match std::fs::read_dir(&self.notes_dir) {
            Ok(d) => d,
            Err(_) => return entries,
        };

        for entry in dir.flatten() {
            let path = entry.path();

            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(f) => f.to_string(),
                None => continue,
            };
            if !filename.ends_with(".md") {
                continue;
            }

            if let Some(mem_entry) = self.parse_memory_file(&path) {
                // Filter by date range
                if let Some(ref f) = from {
                    if mem_entry.date < *f {
                        continue;
                    }
                }
                if let Some(ref t) = to {
                    if mem_entry.date > *t {
                        continue;
                    }
                }

                // Filter by tags (entry must contain at least one of the requested tags)
                if let Some(filter_tags) = tags {
                    if !filter_tags.is_empty()
                        && !filter_tags
                            .iter()
                            .any(|ft| mem_entry.tags.contains(ft))
                    {
                        continue;
                    }
                }

                entries.push(mem_entry);
            }
        }

        // Sort by date ascending
        entries.sort_by_key(|e| e.date);
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a MemoryStore backed by a temp directory.
    fn make_store() -> (TempDir, MemoryStore) {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::new_with_root(tmp.path());
        (tmp, store)
    }

    /// Helper: write a daily note file into the notes subdir (memory/YYYY-MM-DD.md).
    fn write_note(store: &MemoryStore, filename: &str, content: &str) {
        let path = store.notes_dir().join(filename);
        std::fs::write(&path, content).unwrap();
    }

    // ── parse_front_matter tests ──────────────────────────────────────

    #[test]
    fn test_parse_front_matter_inline_tags() {
        let raw = "---\ntags: [meeting, project-x]\n---\n\n# 2025-01-01\n\nSome notes.";
        let (tags, content) = parse_front_matter(raw);
        assert_eq!(tags, vec!["meeting", "project-x"]);
        assert!(content.contains("# 2025-01-01"));
        assert!(content.contains("Some notes."));
        // Front matter should NOT appear in content
        assert!(!content.contains("tags:"));
    }

    #[test]
    fn test_parse_front_matter_list_tags() {
        let raw = "---\ntags:\n- alpha\n- beta\n---\n\nContent here.";
        let (tags, content) = parse_front_matter(raw);
        assert_eq!(tags, vec!["alpha", "beta"]);
        assert!(content.contains("Content here."));
    }

    #[test]
    fn test_parse_front_matter_no_front_matter() {
        let raw = "# Just a heading\n\nNo front matter here.";
        let (tags, content) = parse_front_matter(raw);
        assert!(tags.is_empty());
        assert_eq!(content, raw);
    }

    #[test]
    fn test_parse_front_matter_empty_tags() {
        let raw = "---\ntags: []\n---\n\nContent.";
        let (tags, content) = parse_front_matter(raw);
        assert!(tags.is_empty());
        assert!(content.contains("Content."));
    }

    #[test]
    fn test_parse_front_matter_no_closing_delimiter() {
        let raw = "---\ntags: [oops]\nNo closing delimiter.";
        let (tags, content) = parse_front_matter(raw);
        // Without closing `---`, treat as no front matter
        assert!(tags.is_empty());
        assert_eq!(content, raw);
    }

    // ── date_from_filename tests ──────────────────────────────────────

    #[test]
    fn test_date_from_filename_valid() {
        let d = date_from_filename("2025-07-15.md").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2025, 7, 15).unwrap());
    }

    #[test]
    fn test_date_from_filename_invalid() {
        assert!(date_from_filename("MEMORY.md").is_none());
        assert!(date_from_filename("notes.txt").is_none());
        assert!(date_from_filename("not-a-date.md").is_none());
    }

    // ── MemoryEntry / parse_memory_file tests ─────────────────────────

    #[test]
    fn test_parse_memory_file_with_tags() {
        let (_tmp, store) = make_store();
        write_note(
            &store,
            "2025-03-10.md",
            "---\ntags: [work, rust]\n---\n\n# 2025-03-10\n\nDid some Rust work.",
        );

        let entry = store
            .parse_memory_file(&store.notes_dir().join("2025-03-10.md"))
            .unwrap();
        assert_eq!(entry.date, NaiveDate::from_ymd_opt(2025, 3, 10).unwrap());
        assert_eq!(entry.tags, vec!["work", "rust"]);
        assert!(entry.content.contains("Did some Rust work."));
        assert!(!entry.content.contains("tags:"));
    }

    #[test]
    fn test_parse_memory_file_without_tags() {
        let (_tmp, store) = make_store();
        write_note(
            &store,
            "2025-06-01.md",
            "# 2025-06-01\n\nPlain note without front matter.",
        );

        let entry = store
            .parse_memory_file(&store.notes_dir().join("2025-06-01.md"))
            .unwrap();
        assert_eq!(entry.date, NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
        assert!(entry.tags.is_empty());
        assert!(entry.content.contains("Plain note"));
    }

    // ── search tests ──────────────────────────────────────────────────

    #[test]
    fn test_search_all() {
        let (_tmp, store) = make_store();
        write_note(&store, "2025-01-01.md", "---\ntags: [a]\n---\nNote 1");
        write_note(&store, "2025-01-02.md", "---\ntags: [b]\n---\nNote 2");
        write_note(&store, "2025-01-03.md", "Note 3 no tags");

        let results = store.search(None, None, None);
        assert_eq!(results.len(), 3);
        // Should be sorted by date
        assert_eq!(
            results[0].date,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
        );
        assert_eq!(
            results[2].date,
            NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()
        );
    }

    #[test]
    fn test_search_by_date_range() {
        let (_tmp, store) = make_store();
        write_note(&store, "2025-01-01.md", "Note 1");
        write_note(&store, "2025-01-05.md", "Note 5");
        write_note(&store, "2025-01-10.md", "Note 10");

        let from = NaiveDate::from_ymd_opt(2025, 1, 3);
        let to = NaiveDate::from_ymd_opt(2025, 1, 7);
        let results = store.search(from, to, None);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].date,
            NaiveDate::from_ymd_opt(2025, 1, 5).unwrap()
        );
    }

    #[test]
    fn test_search_by_tags() {
        let (_tmp, store) = make_store();
        write_note(
            &store,
            "2025-02-01.md",
            "---\ntags: [meeting, project-x]\n---\nMeeting notes",
        );
        write_note(
            &store,
            "2025-02-02.md",
            "---\ntags: [personal]\n---\nPersonal stuff",
        );
        write_note(&store, "2025-02-03.md", "No tags here");

        let tags = vec!["meeting".to_string()];
        let results = store.search(None, None, Some(&tags));
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].date,
            NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()
        );
    }

    #[test]
    fn test_search_by_date_and_tags() {
        let (_tmp, store) = make_store();
        write_note(
            &store,
            "2025-03-01.md",
            "---\ntags: [work]\n---\nWork note 1",
        );
        write_note(
            &store,
            "2025-03-15.md",
            "---\ntags: [work]\n---\nWork note 2",
        );
        write_note(
            &store,
            "2025-03-20.md",
            "---\ntags: [personal]\n---\nPersonal note",
        );

        let from = NaiveDate::from_ymd_opt(2025, 3, 10);
        let to = NaiveDate::from_ymd_opt(2025, 3, 25);
        let tags = vec!["work".to_string()];
        let results = store.search(from, to, Some(&tags));
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].date,
            NaiveDate::from_ymd_opt(2025, 3, 15).unwrap()
        );
    }

    #[test]
    fn test_search_skips_memory_md() {
        let (_tmp, store) = make_store();
        std::fs::write(store.memory_file(), "Long-term memory content").unwrap();
        write_note(&store, "2025-04-01.md", "Daily note");

        let results = store.search(None, None, None);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_empty_directory() {
        let (_tmp, store) = make_store();
        let results = store.search(None, None, None);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_tags_filter_returns_all() {
        let (_tmp, store) = make_store();
        write_note(
            &store,
            "2025-05-01.md",
            "---\ntags: [a]\n---\nNote with tag",
        );
        write_note(&store, "2025-05-02.md", "Note without tag");

        // Empty tags slice should return all entries
        let empty_tags: Vec<String> = vec![];
        let results = store.search(None, None, Some(&empty_tags));
        assert_eq!(results.len(), 2);
    }

    // ── get_memory_context_with_window tests ──────────────────────────

    #[test]
    fn test_get_memory_context_with_window_includes_long_term() {
        let (_tmp, store) = make_store();
        std::fs::write(store.memory_file(), "I am long-term memory.").unwrap();

        let ctx = store.get_memory_context_with_window(7);
        assert!(ctx.contains("Long-term Memory"));
        assert!(ctx.contains("I am long-term memory."));
    }

    #[test]
    fn test_get_memory_context_with_window_includes_recent() {
        let (_tmp, store) = make_store();
        let today = Local::now().date_naive();
        let filename = format!("{}.md", today.format("%Y-%m-%d"));
        write_note(&store, &filename, "Today's note content.");

        let ctx = store.get_memory_context_with_window(1);
        assert!(ctx.contains("Recent Notes"));
        assert!(ctx.contains("Today's note content."));
    }

    #[test]
    fn test_get_memory_context_with_window_empty() {
        let (_tmp, store) = make_store();
        let ctx = store.get_memory_context_with_window(7);
        assert!(ctx.is_empty());
    }
}
