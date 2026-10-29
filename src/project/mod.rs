//! Projects — local folders opened as workspaces.
//!
//! A project is a directory the user explicitly opened (a git repo, a docs folder, ...). Once a
//! chat is bound to a project, the agent works inside that folder instead of the global
//! `~/.synbot/workspace`:
//!
//! - filesystem/exec tools resolve relative paths against the project root (via the tool scope),
//! - `ContextBuilder` picks up `<project>/AGENTS.md` and `<project>/.synbot/rules/*.md`,
//! - the UI shows the file tree and the files changed by each turn.
//!
//! State lives in `<config dir>/projects.json` so the selection survives restarts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A directory registered as a project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    /// Stable id derived from the canonical path.
    pub id: String,
    /// Display name (folder name).
    pub name: String,
    /// Canonical absolute path.
    pub path: PathBuf,
    pub added_at: DateTime<Utc>,
    /// Branch/commit recorded when the project was opened (read from `.git`, no git binary).
    pub git_head: Option<String>,
    /// Rule files found under the project root, relative to it.
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProjectFile {
    #[serde(default)]
    projects: Vec<Project>,
    /// `chat_id` → project id, so the web UI reopens the same folder.
    #[serde(default)]
    bindings: HashMap<String, String>,
}

/// Persistent project registry plus per-chat activation.
pub struct ProjectStore {
    path: PathBuf,
    data: ProjectFile,
}

impl ProjectStore {
    /// Default location: `<config dir>/projects.json`.
    pub fn default_path() -> PathBuf {
        crate::config::config_dir().join("projects.json")
    }

    /// Load from `path`; a missing or corrupt file yields an empty registry.
    pub fn load(path: PathBuf) -> Self {
        let data = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<ProjectFile>(&raw).unwrap_or_else(|e| {
                warn!(path = %path.display(), error = %e, "Ignoring unreadable projects.json");
                ProjectFile::default()
            }),
            Err(_) => ProjectFile::default(),
        };
        Self { path, data }
    }

    pub fn with_default_path() -> Self {
        Self::load(Self::default_path())
    }

    pub fn list(&self) -> &[Project] {
        &self.data.projects
    }

    pub fn get(&self, id: &str) -> Option<&Project> {
        self.data.projects.iter().find(|p| p.id == id)
    }

    /// Register (or refresh) a directory and return the project. Re-opening the same path keeps its
    /// id and `added_at`, but refreshes `git_head` and rule discovery.
    pub fn open(&mut self, path: &Path) -> Result<Project> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Project path does not exist: {}", path.display()))?;
        if !canonical.is_dir() {
            anyhow::bail!("Project path is not a directory: {}", canonical.display());
        }
        let name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| canonical.display().to_string());
        let id = project_id(&canonical);
        let rules = discover_rules(&canonical);
        let git_head = read_git_head(&canonical);

        let project = match self.data.projects.iter_mut().find(|p| p.id == id) {
            Some(existing) => {
                existing.name = name;
                existing.path = canonical;
                existing.rules = rules;
                existing.git_head = git_head;
                existing.clone()
            }
            None => {
                let project = Project {
                    id,
                    name,
                    path: canonical,
                    added_at: Utc::now(),
                    git_head,
                    rules,
                };
                self.data.projects.push(project.clone());
                project
            }
        };
        self.persist()?;
        Ok(project)
    }

    /// Forget a project (and any chat bound to it).
    pub fn close(&mut self, id: &str) -> bool {
        let before = self.data.projects.len();
        self.data.projects.retain(|p| p.id != id);
        self.data.bindings.retain(|_, bound| bound != id);
        let removed = self.data.projects.len() != before;
        if removed {
            let _ = self.persist();
        }
        removed
    }

    /// Bind a chat to a project (`None` unbinds it).
    pub fn bind(&mut self, chat_id: &str, project_id: Option<&str>) -> Result<Option<Project>> {
        match project_id {
            Some(id) => {
                let project = self
                    .get(id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Unknown project id: {id}"))?;
                self.data
                    .bindings
                    .insert(chat_id.to_string(), id.to_string());
                self.persist()?;
                Ok(Some(project))
            }
            None => {
                self.data.bindings.remove(chat_id);
                self.persist()?;
                Ok(None)
            }
        }
    }

    /// Project currently bound to `chat_id`, if any.
    pub fn project_for_chat(&self, chat_id: &str) -> Option<Project> {
        let id = self.data.bindings.get(chat_id)?;
        self.get(id).cloned()
    }

    /// Workspace root for `chat_id`: the bound project, else `None`.
    pub fn workspace_for_chat(&self, chat_id: &str) -> Option<PathBuf> {
        self.project_for_chat(chat_id)
            .map(|p| p.path)
            .filter(|p| p.is_dir())
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.path, json)
            .with_context(|| format!("Failed to write {}", self.path.display()))?;
        debug!(path = %self.path.display(), "projects.json saved");
        Ok(())
    }
}

/// Stable id for a canonical path (FNV-ish 64-bit hash, hex encoded).
fn project_id(canonical: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Rule files that should be injected into the system prompt, relative to the project root.
///
/// `<root>/AGENTS.md` first (the repo-level contract), then every `<root>/.synbot/rules/*.md`
/// sorted by name so the prompt is deterministic.
pub fn discover_rules(root: &Path) -> Vec<String> {
    let mut rules = Vec::new();
    if root.join("AGENTS.md").is_file() {
        rules.push("AGENTS.md".to_string());
    }
    let rules_dir = root.join(".synbot").join("rules");
    if let Ok(entries) = std::fs::read_dir(&rules_dir) {
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect();
        names.sort();
        rules.extend(names.into_iter().map(|n| format!(".synbot/rules/{n}")));
    }
    rules
}

/// Read `discover_rules` files, skipping the ones that cannot be read as UTF-8.
pub fn read_rules(root: &Path, max_bytes: usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut budget = max_bytes;
    for rel in discover_rules(root) {
        if budget == 0 {
            break;
        }
        let path = root.join(&rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let content = if content.len() > budget {
            let mut end = budget;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
            budget = 0;
            format!("{}...[truncated]", &content[..end])
        } else {
            budget -= content.len();
            content
        };
        out.push((rel, content));
    }
    out
}

/// Current branch (or detached commit) of a repository, without shelling out to `git`.
fn read_git_head(root: &Path) -> Option<String> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        // Worktrees / submodules keep a `.git` file pointing at the real directory.
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let target = PathBuf::from(target);
        if target.is_absolute() {
            target
        } else {
            root.join(target)
        }
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: ") {
        Some(r) => std::fs::read_to_string(git_dir.join(r))
            .ok()
            .map(|sha| sha.trim().to_string())
            .or_else(|| Some(r.to_string())),
        None => Some(head.chars().take(12).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> ProjectStore {
        ProjectStore::load(dir.join("projects.json"))
    }

    #[test]
    fn open_registers_project_and_discovers_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("my-repo");
        std::fs::create_dir_all(root.join(".synbot/rules")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# rules").unwrap();
        std::fs::write(root.join(".synbot/rules/b.md"), "b").unwrap();
        std::fs::write(root.join(".synbot/rules/a.md"), "a").unwrap();
        std::fs::write(root.join(".synbot/rules/notes.txt"), "ignored").unwrap();

        let mut store = store(tmp.path());
        let project = store.open(&root).unwrap();

        assert_eq!(project.name, "my-repo");
        assert_eq!(
            project.rules,
            vec![
                "AGENTS.md".to_string(),
                ".synbot/rules/a.md".to_string(),
                ".synbot/rules/b.md".to_string()
            ]
        );
        assert_eq!(store.list().len(), 1);
        // Opening the same folder twice keeps a single entry.
        store.open(&root).unwrap();
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn open_rejects_missing_and_file_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = store(tmp.path());
        assert!(store.open(&tmp.path().join("nope")).is_err());

        let file = tmp.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(store.open(&file).is_err());
    }

    #[test]
    fn bind_and_reload_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();

        let mut store = store(tmp.path());
        let project = store.open(&root).unwrap();
        store.bind("web_admin", Some(&project.id)).unwrap();
        assert_eq!(
            store.workspace_for_chat("web_admin").unwrap(),
            root.canonicalize().unwrap()
        );

        let reloaded = ProjectStore::load(tmp.path().join("projects.json"));
        assert_eq!(
            reloaded.project_for_chat("web_admin").map(|p| p.id),
            Some(project.id.clone())
        );

        let mut store = reloaded;
        assert!(store.bind("web_admin", None).unwrap().is_none());
        assert!(store.workspace_for_chat("web_admin").is_none());
    }

    #[test]
    fn binding_unknown_project_is_an_error_and_close_clears_bindings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = store(tmp.path());
        assert!(store.bind("chat", Some("missing")).is_err());

        let project = store.open(&root).unwrap();
        store.bind("chat", Some(&project.id)).unwrap();
        assert!(store.close(&project.id));
        assert!(store.project_for_chat("chat").is_none());
        assert!(!store.close(&project.id));
    }

    #[test]
    fn workspace_for_chat_is_none_when_folder_disappeared() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = store(tmp.path());
        let project = store.open(&root).unwrap();
        store.bind("chat", Some(&project.id)).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(store.project_for_chat("chat").is_some());
        assert!(store.workspace_for_chat("chat").is_none());
    }

    #[test]
    fn read_rules_respects_budget() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "1234567890").unwrap();
        let rules = read_rules(tmp.path(), 4);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].1.ends_with("[truncated]"));
    }
}
