//! Checkpoints — a pre-write snapshot of every file a turn touches.
//!
//! The agent loop captures the file's content *before* a mutating tool runs and the new content
//! afterwards, then stores both under `<config dir>/checkpoints/<turn_id>.json`. That gives the UI
//! two things it could not do before:
//!
//! - **Review**: a structured line diff per file, clickable from the "changed files" list.
//! - **Undo**: restore the whole turn (created files removed, modified files rolled back,
//!   renames reverted) without touching anything the turn did not write.
//!
//! Only filesystem tools are covered (`write_file`, `edit_file`, `create_dir`, `move_file`).
//! Writes done through `exec` (git, formatters, generators) are invisible here — they still show up
//! as reported by the tool, but cannot be rolled back from a checkpoint.
//!
//! Snapshots are capped per file: larger (`MAX_SNAPSHOT_BYTES`) or binary files are recorded with a
//! `skipped` reason so the diff view can explain why it shows nothing and undo refuses that file.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Files bigger than this are not snapshotted (keeps `checkpoints/*.json` small).
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;
/// How many turn checkpoints to keep on disk.
const MAX_CHECKPOINTS: usize = 200;
/// Diff guard: beyond this many line pairs fall back to "delete all / add all".
const MAX_DIFF_CELLS: usize = 4_000_000;
/// Context lines kept around each change.
const DIFF_CONTEXT: usize = 3;

/// How a file changed during a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// One file captured by a checkpoint (pre-image, post-image and how to undo it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointFile {
    /// Absolute path the tool used.
    pub path: String,
    /// Path relative to the turn's workspace (falls back to `path`).
    pub display_path: String,
    pub change: ChangeKind,
    /// Rename target, only for [`ChangeKind::Renamed`].
    pub renamed_to: Option<String>,
    /// Content before the tool ran (`None` = the file did not exist).
    pub before: Option<String>,
    /// Content after the tool ran (`None` = the file no longer exists).
    pub after: Option<String>,
    /// Why this file could not be snapshotted (too large / binary / unreadable).
    pub skipped: Option<String>,
}

/// A whole turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Turn id (`AgentEvent::TurnStarted.turn_id`), also the file name.
    pub id: String,
    pub created_at: DateTime<Utc>,
    /// Root the turn worked in, used to render relative paths.
    pub workspace: String,
    pub files: Vec<CheckpointFile>,
    pub undone_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub undo_skipped: Vec<String>,
}

/// API view of a checkpoint: adds computed diffs and counts.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointView {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub workspace: String,
    pub undone_at: Option<DateTime<Utc>>,
    pub undo_skipped: Vec<String>,
    pub files: Vec<FileView>,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileView {
    pub path: String,
    pub display_path: String,
    pub change: ChangeKind,
    pub renamed_to: Option<String>,
    pub skipped: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// `"ctx"`, `"add"` or `"del"`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_no: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UndoReport {
    pub turn_id: String,
    /// Files restored / removed.
    pub restored: Vec<String>,
    /// Files that could not be restored (not snapshotted, or the write failed).
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// Pre-images captured before a tool ran; completed by [`CheckpointStore::finish`].
#[derive(Debug, Clone)]
pub struct PendingSnapshot {
    turn_id: String,
    workspace: PathBuf,
    files: Vec<CheckpointFile>,
}

impl PendingSnapshot {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Turn checkpoints on disk.
pub struct CheckpointStore {
    root: PathBuf,
}

impl CheckpointStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `<config dir>/checkpoints`.
    pub fn default_root() -> PathBuf {
        crate::config::config_dir().join("checkpoints")
    }

    pub fn with_default_root() -> Self {
        Self::new(Self::default_root())
    }

    /// Directory holding one JSON file per turn.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file_for(&self, turn_id: &str) -> PathBuf {
        // Turn ids are UUIDs; guard anyway so a crafted id cannot escape the directory.
        let safe: String = turn_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.root.join(format!("{safe}.json"))
    }

    /// Snapshot the files this tool call is about to touch.
    ///
    /// Paths are resolved exactly like the filesystem tools resolve them: relative to the active
    /// tool scope (`ToolContext::workspace`), which is the project root when the chat is bound to a
    /// project. Each mutation is its own capture so a turn that writes the same file twice keeps
    /// the first pre-image (the second capture sees the already-updated file and is dropped in
    /// [`Self::finish`]).
    pub async fn capture_before(
        &self,
        turn_id: &str,
        tool: &str,
        args: &Value,
    ) -> Option<PendingSnapshot> {
        let workspace = crate::tools::context::current_allowed_roots()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let targets = targets_for_tool(tool, args, &workspace);
        if targets.is_empty() {
            return None;
        }

        let mut files = Vec::with_capacity(targets.len());
        for target in targets {
            let (before, skipped) = read_snapshot(&target.path);
            files.push(CheckpointFile {
                display_path: display_path(&workspace, &target.path),
                path: target.path.to_string_lossy().to_string(),
                change: target.change,
                renamed_to: target.renamed_to.map(|p| p.to_string_lossy().to_string()),
                before,
                after: None,
                skipped,
            });
        }
        Some(PendingSnapshot {
            turn_id: turn_id.to_string(),
            workspace,
            files,
        })
    }

    /// Read the post-images, merge them into the turn's checkpoint and persist.
    ///
    /// Returns the number of files recorded. Failures are logged, never propagated: a checkpoint
    /// problem must not fail the tool call itself.
    pub async fn finish(&self, pending: PendingSnapshot) -> usize {
        let mut files = pending.files;
        for file in files.iter_mut() {
            // Directory creation has no content; undo removes the (empty) directory.
            let target = match (&file.renamed_to, file.change) {
                (Some(dest), ChangeKind::Renamed) => PathBuf::from(dest),
                _ => PathBuf::from(&file.path),
            };
            if file.skipped.is_some() {
                continue;
            }
            let (after, skipped) = read_snapshot(&target);
            file.after = after;
            if file.skipped.is_none() {
                file.skipped = skipped;
            }
        }

        let mut checkpoint = match self.read(&pending.turn_id) {
            Ok(existing) => existing,
            Err(_) => Checkpoint {
                id: pending.turn_id.clone(),
                created_at: Utc::now(),
                workspace: pending.workspace.to_string_lossy().to_string(),
                files: Vec::new(),
                undone_at: None,
                undo_skipped: Vec::new(),
            },
        };
        for file in files {
            match checkpoint.files.iter_mut().find(|f| f.path == file.path) {
                // Keep the original pre-image; only refresh the post-image.
                Some(existing) => existing.after = file.after,
                None => checkpoint.files.push(file),
            }
        }
        if let Err(e) = self.write(&checkpoint) {
            warn!(turn_id = %pending.turn_id, error = %e, "Failed to persist checkpoint");
            return checkpoint.files.len();
        }
        checkpoint.files.len()
    }

    pub fn get(&self, turn_id: &str) -> Option<Checkpoint> {
        self.read(turn_id).ok()
    }

    pub fn view(&self, turn_id: &str) -> Option<CheckpointView> {
        self.get(turn_id).map(|c| c.view())
    }

    /// Restore every file of the turn. Individual failures are reported, not fatal.
    pub fn undo(&self, turn_id: &str) -> Result<UndoReport> {
        let mut checkpoint = self
            .read(turn_id)
            .with_context(|| format!("Checkpoint not found: {turn_id}"))?;
        let mut report = UndoReport {
            turn_id: turn_id.to_string(),
            restored: Vec::new(),
            skipped: Vec::new(),
            errors: Vec::new(),
        };

        // Reverse order so a create+modify sequence ends with the file removed.
        for file in checkpoint.files.iter().rev() {
            if file.skipped.is_some() {
                report.skipped.push(file.display_path.clone());
                continue;
            }
            let result = match file.change {
                ChangeKind::Created => match std::fs::remove_file(&file.path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    // create_dir recorded a directory: remove it when empty.
                    Err(_) => std::fs::remove_dir(&file.path),
                },
                ChangeKind::Modified | ChangeKind::Deleted => {
                    restore_file(&file.path, file.before.as_deref())
                }
                ChangeKind::Renamed => {
                    let restore_source = restore_file(&file.path, file.before.as_deref());
                    if let Some(dest) = &file.renamed_to {
                        let _ = std::fs::remove_file(dest);
                    }
                    restore_source
                }
            };
            match result {
                Ok(()) => report.restored.push(file.display_path.clone()),
                Err(e) => {
                    report.skipped.push(file.display_path.clone());
                    report.errors.push(format!("{}: {e}", file.display_path));
                }
            }
        }

        checkpoint.undone_at = Some(Utc::now());
        checkpoint.undo_skipped = report.skipped.clone();
        if let Err(e) = self.write(&checkpoint) {
            warn!(turn_id, error = %e, "Failed to persist undo state");
        }
        Ok(report)
    }

    /// Newest checkpoints first, without file payloads.
    pub fn recent(&self, limit: usize) -> Vec<CheckpointSummary> {
        let mut entries: Vec<(std::time::SystemTime, PathBuf)> = match std::fs::read_dir(&self.root)
        {
            Ok(dir) => dir
                .flatten()
                .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|e| {
                    let meta = e.metadata().ok()?;
                    Some((meta.modified().ok()?, e.path()))
                })
                .collect(),
            Err(_) => return Vec::new(),
        };
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        entries
            .into_iter()
            .take(limit)
            .filter_map(|(_, path)| {
                std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| serde_json::from_str::<Checkpoint>(&raw).ok())
                    .map(|c| c.summary())
            })
            .collect()
    }

    fn read(&self, turn_id: &str) -> Result<Checkpoint> {
        let path = self.file_for(turn_id);
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Checkpoint {} not found", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("Corrupt checkpoint {}", path.display()))
    }

    fn write(&self, checkpoint: &Checkpoint) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("Failed to create {}", self.root.display()))?;
        let path = self.file_for(&checkpoint.id);
        let json = serde_json::to_string_pretty(checkpoint)?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        debug!(path = %path.display(), files = checkpoint.files.len(), "checkpoint saved");
        self.prune();
        Ok(())
    }

    /// Keep only the newest [`MAX_CHECKPOINTS`] files. Cheap enough to run per save.
    fn prune(&self) {
        let Ok(dir) = std::fs::read_dir(&self.root) else {
            return;
        };
        let mut entries: Vec<(std::time::SystemTime, PathBuf)> = dir
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((meta.modified().ok()?, e.path()))
            })
            .collect();
        if entries.len() <= MAX_CHECKPOINTS {
            return;
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, path) in entries.into_iter().skip(MAX_CHECKPOINTS) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointSummary {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub files: usize,
    pub undone_at: Option<DateTime<Utc>>,
}

impl Checkpoint {
    pub fn summary(&self) -> CheckpointSummary {
        CheckpointSummary {
            id: self.id.clone(),
            created_at: self.created_at,
            files: self.files.len(),
            undone_at: self.undone_at,
        }
    }

    pub fn view(&self) -> CheckpointView {
        let files: Vec<FileView> = self.files.iter().map(FileView::from).collect();
        let additions = files.iter().map(|f| f.additions).sum();
        let deletions = files.iter().map(|f| f.deletions).sum();
        CheckpointView {
            id: self.id.clone(),
            created_at: self.created_at,
            workspace: self.workspace.clone(),
            undone_at: self.undone_at,
            undo_skipped: self.undo_skipped.clone(),
            files,
            additions,
            deletions,
        }
    }
}

impl From<&CheckpointFile> for FileView {
    fn from(file: &CheckpointFile) -> Self {
        let (hunks, additions, deletions) = match (file.change, &file.before, &file.after) {
            (_, Some(before), Some(after)) => diff(before, after),
            (ChangeKind::Created, None, Some(after)) => diff("", after),
            (ChangeKind::Deleted, Some(before), None) => diff(before, ""),
            // Binary / oversized: nothing to show.
            _ => (Vec::new(), 0, 0),
        };
        FileView {
            path: file.path.clone(),
            display_path: file.display_path.clone(),
            change: file.change,
            renamed_to: file.renamed_to.clone(),
            skipped: file.skipped.clone(),
            additions,
            deletions,
            hunks,
        }
    }
}

struct Target {
    path: PathBuf,
    change: ChangeKind,
    renamed_to: Option<PathBuf>,
}

/// Files a mutating filesystem tool touches. Kept in sync with the `FileChanged` mapping in the
/// agent loop (same four tools).
fn targets_for_tool(tool: &str, args: &Value, workspace: &Path) -> Vec<Target> {
    let resolve = |key: &str| -> Option<PathBuf> {
        let raw = args.get(key)?.as_str()?.trim();
        if raw.is_empty() {
            return None;
        }
        let path = Path::new(raw);
        Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace.join(path)
        })
    };

    match tool {
        "write_file" | "edit_file" => resolve("path")
            .map(|path| {
                let exists = path.exists();
                vec![Target {
                    change: if exists {
                        ChangeKind::Modified
                    } else {
                        ChangeKind::Created
                    },
                    path,
                    renamed_to: None,
                }]
            })
            .unwrap_or_default(),
        "create_dir" => resolve("path")
            .map(|path| {
                vec![Target {
                    path,
                    change: ChangeKind::Created,
                    renamed_to: None,
                }]
            })
            .unwrap_or_default(),
        "move_file" => match (resolve("source"), resolve("destination")) {
            (Some(source), Some(destination)) => vec![Target {
                path: source,
                change: ChangeKind::Renamed,
                renamed_to: Some(destination),
            }],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Read a file for snapshotting, explaining why it was skipped when it cannot be used.
fn read_snapshot(path: &Path) -> (Option<String>, Option<String>) {
    if path.is_dir() {
        return (None, None);
    }
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(e) => return (None, Some(format!("unreadable: {e}"))),
    };
    if meta.len() as usize > MAX_SNAPSHOT_BYTES {
        return (None, Some(format!("too large ({} bytes)", meta.len())));
    }
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => (Some(text), None),
            Err(_) => (None, Some("binary file".to_string())),
        },
        Err(e) => (None, Some(format!("unreadable: {e}"))),
    }
}

fn restore_file(path: &str, before: Option<&str>) -> std::io::Result<()> {
    match before {
        Some(content) => {
            let target = Path::new(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, content)
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
    }
}

/// Path as shown in the UI: relative to the turn's workspace when inside it.
fn display_path(workspace: &Path, path: &Path) -> String {
    let ws = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    match canonical.strip_prefix(&ws) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

/// Line diff with hunks. Falls back to "all removed / all added" for very large inputs.
fn diff(before: &str, after: &str) -> (Vec<DiffHunk>, u32, u32) {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    if old.len().saturating_mul(new.len()) > MAX_DIFF_CELLS {
        let lines = old
            .iter()
            .enumerate()
            .map(|(i, text)| DiffLine {
                kind: "del",
                old_no: Some(i as u32 + 1),
                new_no: None,
                text: text.to_string(),
            })
            .chain(new.iter().enumerate().map(|(i, text)| DiffLine {
                kind: "add",
                old_no: None,
                new_no: Some(i as u32 + 1),
                text: text.to_string(),
            }))
            .collect();
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines,
        };
        return (vec![hunk], new.len() as u32, old.len() as u32);
    }

    // Classic LCS table (u32): cell (i, j) = LCS length of old[i..], new[j..].
    let (n, m) = (old.len(), new.len());
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut ops: Vec<DiffLine> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(DiffLine {
                kind: "ctx",
                old_no: Some(i as u32 + 1),
                new_no: Some(j as u32 + 1),
                text: old[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(DiffLine {
                kind: "del",
                old_no: Some(i as u32 + 1),
                new_no: None,
                text: old[i].to_string(),
            });
            i += 1;
        } else {
            ops.push(DiffLine {
                kind: "add",
                old_no: None,
                new_no: Some(j as u32 + 1),
                text: new[j].to_string(),
            });
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffLine {
            kind: "del",
            old_no: Some(i as u32 + 1),
            new_no: None,
            text: old[i].to_string(),
        });
        i += 1;
    }
    while j < m {
        ops.push(DiffLine {
            kind: "add",
            old_no: None,
            new_no: Some(j as u32 + 1),
            text: new[j].to_string(),
        });
        j += 1;
    }

    let additions = ops.iter().filter(|l| l.kind == "add").count() as u32;
    let deletions = ops.iter().filter(|l| l.kind == "del").count() as u32;
    (group_hunks(&ops), additions, deletions)
}

/// Group flat diff lines into hunks, keeping [`DIFF_CONTEXT`] lines of context.
fn group_hunks(ops: &[DiffLine]) -> Vec<DiffHunk> {
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != "ctx")
        .map(|(i, _)| i)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &index in &changed {
        let start = index.saturating_sub(DIFF_CONTEXT);
        let end = (index + DIFF_CONTEXT).min(ops.len() - 1);
        match ranges.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => ranges.push((start, end)),
        }
    }

    ranges
        .into_iter()
        .map(|(start, end)| {
            let lines: Vec<DiffLine> = ops[start..=end].to_vec();
            DiffHunk {
                old_start: lines
                    .iter()
                    .find_map(|l| l.old_no)
                    .unwrap_or(start as u32 + 1),
                new_start: lines
                    .iter()
                    .find_map(|l| l.new_no)
                    .unwrap_or(start as u32 + 1),
                lines,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store(dir: &Path) -> CheckpointStore {
        CheckpointStore::new(dir.join("checkpoints"))
    }

    /// Run `tool` with `args` inside a workspace scope, capturing like the agent loop does.
    async fn capture(
        store: &CheckpointStore,
        turn: &str,
        workspace: &Path,
        tool: &str,
        args: Value,
        write: impl FnOnce(),
    ) -> usize {
        let ctx = crate::tools::context::ToolContext {
            agent_id: "main".to_string(),
            workspace: workspace.to_path_buf(),
        };
        let pending = crate::tools::context::scope(ctx, async {
            store.capture_before(turn, tool, &args).await
        })
        .await
        .expect("tool target is captured inside the scope");
        write();
        store.finish(pending).await
    }

    #[tokio::test]
    async fn captures_modified_file_and_undoes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let file = tmp.path().join("notes.md");
        std::fs::write(&file, "one\ntwo\n").unwrap();

        let recorded = capture(
            &store,
            "turn-1",
            tmp.path(),
            "write_file",
            json!({ "path": "notes.md", "content": "one\n2\n" }),
            || std::fs::write(&file, "one\n2\n").unwrap(),
        )
        .await;
        assert_eq!(recorded, 1);

        let view = store.view("turn-1").unwrap();
        assert_eq!(view.additions, 1);
        assert_eq!(view.deletions, 1);
        assert_eq!(view.files[0].display_path, "notes.md");

        let report = store.undo("turn-1").unwrap();
        assert_eq!(report.restored, vec!["notes.md".to_string()]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\ntwo\n");
    }

    #[tokio::test]
    async fn created_file_is_removed_on_undo() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let file = tmp.path().join("new/hello.txt");

        capture(
            &store,
            "turn-new",
            tmp.path(),
            "write_file",
            json!({ "path": "new/hello.txt", "content": "hi" }),
            || {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(&file, "hi").unwrap();
            },
        )
        .await;

        let view = store.view("turn-new").unwrap();
        assert_eq!(view.files[0].change, ChangeKind::Created);
        assert_eq!(view.additions, 1);

        store.undo("turn-new").unwrap();
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn rename_is_reverted_and_duplicate_writes_keep_first_pre_image() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let source = tmp.path().join("a.txt");
        let dest = tmp.path().join("b.txt");
        std::fs::write(&source, "v1").unwrap();

        capture(
            &store,
            "turn-move",
            tmp.path(),
            "move_file",
            json!({ "source": "a.txt", "destination": "b.txt" }),
            || std::fs::rename(&source, &dest).unwrap(),
        )
        .await;

        // A second write to the same file in one turn must not overwrite the first pre-image.
        capture(
            &store,
            "turn-move",
            tmp.path(),
            "write_file",
            json!({ "path": "b.txt", "content": "v3" }),
            || std::fs::write(&dest, "v3").unwrap(),
        )
        .await;

        let checkpoint = store.get("turn-move").unwrap();
        let moved = checkpoint
            .files
            .iter()
            .find(|f| f.change == ChangeKind::Renamed)
            .unwrap();
        assert_eq!(moved.before.as_deref(), Some("v1"));
        // The later write to the renamed file is its own entry, so the rename's pre-image stays.
        let rewritten = checkpoint
            .files
            .iter()
            .find(|f| f.path.ends_with("b.txt"))
            .unwrap();
        assert_eq!(rewritten.before.as_deref(), Some("v1"));

        store.undo("turn-move").unwrap();
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "v1");
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn oversized_and_binary_files_are_skipped_not_undone() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let big = tmp.path().join("big.bin");
        std::fs::write(&big, vec![0u8; MAX_SNAPSHOT_BYTES + 1]).unwrap();

        capture(
            &store,
            "turn-big",
            tmp.path(),
            "write_file",
            json!({ "path": "big.bin", "content": "x" }),
            || {},
        )
        .await;

        let view = store.view("turn-big").unwrap();
        assert!(view.files[0].skipped.is_some());

        let report = store.undo("turn-big").unwrap();
        assert_eq!(report.restored.len(), 0);
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn diff_groups_hunks_with_context() {
        let before = (1..=20).map(|i| format!("line{i}\n")).collect::<String>();
        let mut after_lines: Vec<String> = (1..=20).map(|i| format!("line{i}")).collect();
        after_lines[9] = "changed".to_string();
        let after = after_lines.join("\n") + "\n";

        let (hunks, additions, deletions) = diff(&before, &after);
        assert_eq!((additions, deletions), (1, 1));
        assert_eq!(hunks.len(), 1);
        // 3 context lines on both sides of the del+add pair.
        assert_eq!(hunks[0].lines.len(), 8);
        assert_eq!(hunks[0].old_start, 7);
    }

    #[test]
    fn diff_of_identical_content_is_empty() {
        let (hunks, additions, deletions) = diff("a\nb\n", "a\nb\n");
        assert!(hunks.is_empty());
        assert_eq!((additions, deletions), (0, 0));
    }

    #[test]
    fn unknown_tools_have_no_targets() {
        let args = json!({ "path": "x" });
        assert!(targets_for_tool("read_file", &args, Path::new("/tmp")).is_empty());
        assert!(targets_for_tool("exec", &args, Path::new("/tmp")).is_empty());
    }
}
