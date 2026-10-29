//! Git introspection shared by the `git_*` tools and the `/api/git/*` routes.
//!
//! Everything here is **read-only**: `git status`, `git diff`, `git log` and a couple of cheap
//! rev-parse calls. Mutating git operations (add/commit/branch/checkout) stay in `exec`, which
//! already carries the approval flow, the command policy and the audit trail — duplicating that
//! machinery behind a new tool would weaken it.
//!
//! All commands run with `current_dir(repo)` and no shell, so a path in the arguments can never
//! turn into shell injection.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Command;

/// Budget for one `git diff` result (keeps tool output and HTTP payloads small).
pub const MAX_DIFF_LINES: usize = 400;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitFile {
    /// Path relative to the repository root, `/`-separated.
    pub path: String,
    /// Porcelain status letters (`M`, `A`, `D`, `R`, `??`, …).
    pub status: String,
    /// Index side (staged) vs worktree side.
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct GitStatus {
    /// False when the folder is not a git work tree (or git is not installed).
    pub is_repo: bool,
    pub branch: Option<String>,
    /// Commits ahead of the upstream branch.
    pub ahead: u32,
    /// Commits behind the upstream branch.
    pub behind: u32,
    /// `HEAD` short hash.
    pub head: Option<String>,
    pub files: Vec<GitFile>,
}

impl GitStatus {
    pub fn staged(&self) -> impl Iterator<Item = &GitFile> {
        self.files.iter().filter(|f| f.staged)
    }

    pub fn unstaged(&self) -> impl Iterator<Item = &GitFile> {
        self.files.iter().filter(|f| !f.staged)
    }

    /// One-line summary for prompts and the UI (`main ↑1 ↓0 · 3 changed`).
    pub fn summary(&self) -> String {
        if !self.is_repo {
            return "not a git repository".to_string();
        }
        let branch = self
            .branch
            .clone()
            .unwrap_or_else(|| "(detached)".to_string());
        let mut out = branch;
        if self.ahead > 0 {
            out.push_str(&format!(" ↑{}", self.ahead));
        }
        if self.behind > 0 {
            out.push_str(&format!(" ↓{}", self.behind));
        }
        if !self.files.is_empty() {
            out.push_str(&format!(" · {} changed", self.files.len()));
        } else {
            out.push_str(" · clean");
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

/// Run a git command in `root`, returning stdout on success.
fn run_git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run `git {}`", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`git {}` failed in {}: {}",
            args.join(" "),
            root.display(),
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Whether `root` is inside a git work tree.
pub fn is_repo(root: &Path) -> bool {
    root.is_dir()
        && Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false)
}

/// `git status --porcelain=v1 -b`, parsed into a branch header + per-file entries.
///
/// A folder that is not a repository is not an error: it returns `is_repo: false` so the UI can
/// simply hide the git strip and the tools can tell the model git is unavailable.
pub fn status(root: &Path) -> Result<GitStatus> {
    if !is_repo(root) {
        return Ok(GitStatus::default());
    }
    let raw = run_git(root, &["status", "--porcelain=v1", "-b"])?;
    let mut out = GitStatus {
        is_repo: true,
        ..GitStatus::default()
    };
    for line in raw.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            parse_branch_header(header, &mut out);
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let index = &line[0..1];
        let worktree = &line[1..2];
        let path = line[3..].to_string();
        // `R  old -> new` reports both sides; keep the destination (what exists on disk now).
        let path = match path.split_once(" -> ") {
            Some((_, new)) => new.to_string(),
            None => path,
        };
        if index == "?" || worktree == "?" {
            out.files.push(GitFile {
                path,
                status: "??".to_string(),
                staged: false,
            });
            continue;
        }
        if index != " " {
            out.files.push(GitFile {
                path: path.clone(),
                status: index.to_string(),
                staged: true,
            });
        }
        if worktree != " " {
            out.files.push(GitFile {
                path,
                status: worktree.to_string(),
                staged: false,
            });
        }
    }
    // `HEAD` is nicer as a short hash than digging it out of the header.
    if let Ok(head) = run_git(root, &["rev-parse", "--short", "HEAD"]) {
        let head = head.trim();
        if !head.is_empty() {
            out.head = Some(head.to_string());
        }
    }
    Ok(out)
}

fn parse_branch_header(header: &str, out: &mut GitStatus) {
    // `main...origin/main [ahead 2, behind 1]`, `No commits yet on main`, `HEAD (no branch)`.
    let (name, rest) = match header.split_once("...") {
        Some((name, rest)) => (name, rest),
        None => (header, ""),
    };
    let name = name.trim();
    if name == "No commits yet on" {
        out.branch = header.split_whitespace().last().map(|s| s.to_string());
    } else if name == "HEAD (no branch)" || name.is_empty() {
        out.branch = Some("HEAD".to_string());
    } else {
        out.branch = Some(name.to_string());
    }
    if let Some(start) = rest.find("[") {
        if let Some(end) = rest.rfind(']') {
            for part in rest[start + 1..end].split(',') {
                let part = part.trim();
                if let Some(n) = part.strip_prefix("ahead ") {
                    out.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    out.behind = n.parse().unwrap_or(0);
                }
            }
        }
    }
}

/// `git diff` (worktree) or `git diff --cached` (index), optionally limited to one path.
pub fn diff(root: &Path, path: Option<&str>, staged: bool, max_lines: usize) -> Result<String> {
    if !is_repo(root) {
        anyhow::bail!("{} is not a git repository", root.display());
    }
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    // `--` guards against a path being read as a revision.
    if let Some(path) = path {
        args.push("--");
        args.push(path);
    }
    let raw = run_git(root, &args)?;
    if raw.trim().is_empty() {
        return Ok(if staged {
            "(no staged changes)".to_string()
        } else {
            "(no unstaged changes)".to_string()
        });
    }
    Ok(truncate_lines(&raw, max_lines))
}

/// Newest commits, newest first.
pub fn log(root: &Path, limit: usize, path: Option<&str>) -> Result<Vec<GitCommit>> {
    if !is_repo(root) {
        anyhow::bail!("{} is not a git repository", root.display());
    }
    let limit = limit.clamp(1, 200).to_string();
    let mut args = vec![
        "log",
        "--no-color",
        "--date=short",
        "--pretty=format:%h\x1f%an\x1f%ad\x1f%s",
        "-n",
        &limit,
    ];
    if let Some(path) = path {
        args.push("--");
        args.push(path);
    }
    let raw = run_git(root, &args)?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\x1f');
            Some(GitCommit {
                hash: parts.next()?.to_string(),
                author: parts.next()?.to_string(),
                date: parts.next()?.to_string(),
                subject: parts.next().unwrap_or_default().to_string(),
            })
        })
        .collect())
}

/// Cap a long diff, saying so instead of silently cutting it.
fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let omitted = lines.len() - max_lines;
    format!(
        "{}\n... [{omitted} more lines truncated — pass `path` to narrow the diff]",
        lines[..max_lines].join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// A throwaway repo with one commit and a configured identity.
    fn repo() -> Option<tempfile::TempDir> {
        if !git_available() {
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            Command::new("git")
                .current_dir(root)
                .args(&args)
                .output()
                .unwrap();
        }
        std::fs::write(root.join("a.txt"), "one\n").unwrap();
        Command::new("git")
            .current_dir(root)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(root)
            .args(["commit", "-q", "-m", "first commit"])
            .output()
            .unwrap();
        Some(dir)
    }

    #[test]
    fn non_repository_is_reported_not_failed() {
        let dir = tempfile::tempdir().unwrap();
        let status = status(dir.path()).unwrap();
        assert!(!status.is_repo);
        assert_eq!(status.summary(), "not a git repository");
    }

    #[test]
    fn status_lists_staged_unstaged_and_untracked() {
        let Some(dir) = repo() else { return };
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(root.join("b.txt"), "new\n").unwrap();

        let status = status(root).unwrap();
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(status.head.is_some());
        let paths: Vec<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"b.txt"));
        assert!(status.files.iter().any(|f| f.status == "??"));
        assert!(status.summary().contains("changed"));
    }

    #[test]
    fn diff_and_log_report_the_commit() {
        let Some(dir) = repo() else { return };
        let root = dir.path();
        assert!(diff(root, None, false, MAX_DIFF_LINES)
            .unwrap()
            .contains("no unstaged changes"));

        std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        let patch = diff(root, Some("a.txt"), false, MAX_DIFF_LINES).unwrap();
        assert!(patch.contains("+two"), "patch: {patch}");
        assert!(patch.contains("a.txt"));

        let commits = log(root, 5, None).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "first commit");
    }

    #[test]
    fn long_diffs_are_truncated_with_a_hint() {
        let text = (1..=10)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_lines(&text, 4);
        assert!(out.contains("6 more lines truncated"));
        assert!(out.contains("line4"));
        assert!(!out.contains("line5"));
    }

    #[test]
    fn branch_header_parses_ahead_behind() {
        let mut status = GitStatus::default();
        parse_branch_header("main...origin/main [ahead 2, behind 1]", &mut status);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
    }

    #[test]
    fn diff_outside_a_repo_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = diff(dir.path(), None, false, 10).unwrap_err();
        assert!(err.to_string().contains("not a git repository"));
    }
}
