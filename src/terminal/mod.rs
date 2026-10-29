//! Persistent PTY sessions for the workbench terminal panel.
//!
//! One session = one shell running inside a pseudo-terminal, addressed by an id the UI keeps for as
//! long as the panel is open. Output is buffered in a ring (so a late client can replay what it
//! missed) and read incrementally by an absolute cursor, which keeps the transport simple: the web
//! socket polls, and no output is lost or duplicated when a frame is slow.
//!
//! Sessions are scoped to a workspace folder — the caller (web handler) resolves it from the active
//! project, never from client-supplied paths — and there is a hard cap so a runaway client cannot
//! fork unbounded shells.

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Bytes of scrollback kept per session; older output is dropped and reported as such via `base`.
pub const MAX_BUFFER_BYTES: usize = 256 * 1024;
/// Concurrent shells. Four is plenty for a workbench and keeps a stuck UI from forking a fleet.
pub const MAX_SESSIONS: usize = 4;
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const MAX_DIMENSION: u16 = 500;

#[derive(Debug, Clone, Serialize)]
pub struct TerminalInfo {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
    pub alive: bool,
    /// First byte still buffered (everything below was dropped from the ring).
    pub base: u64,
    /// Absolute offset of the next byte, i.e. the cursor a client should resume from.
    pub cursor: u64,
}

/// Ring buffer with absolute offsets. `base + len` is the cursor.
#[derive(Debug, Default)]
struct OutputBuffer {
    data: VecDeque<u8>,
    base: u64,
}

impl OutputBuffer {
    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= MAX_BUFFER_BYTES {
            let dropped = chunk.len() - MAX_BUFFER_BYTES;
            self.base += dropped as u64 + self.data.len() as u64;
            self.data.clear();
            self.data.extend(&chunk[dropped..]);
            return;
        }
        self.data.extend(chunk);
        while self.data.len() > MAX_BUFFER_BYTES {
            self.data.pop_front();
            self.base += 1;
        }
    }

    fn cursor(&self) -> u64 {
        self.base + self.data.len() as u64
    }

    /// Bytes from `cursor` onwards. A cursor below `base` means the client fell behind the ring:
    /// we return what is still buffered and move the cursor to `base`.
    fn read_from(&self, cursor: u64) -> (Vec<u8>, u64) {
        let start = cursor.max(self.base).min(self.cursor());
        let offset = (start - self.base) as usize;
        let bytes = self.data.iter().skip(offset).copied().collect();
        (bytes, self.cursor())
    }
}

/// Splits a byte stream into valid UTF-8 strings, holding back an incomplete trailing sequence so a
/// multi-byte character split across two PTY reads is not rendered as a replacement character.
#[derive(Debug, Default)]
pub struct Utf8Chunker {
    carry: Vec<u8>,
}

impl Utf8Chunker {
    pub fn push(&mut self, chunk: &[u8]) -> String {
        let mut bytes = std::mem::take(&mut self.carry);
        bytes.extend_from_slice(chunk);
        match std::str::from_utf8(&bytes) {
            Ok(text) => text.to_string(),
            Err(err) => {
                let valid = err.valid_up_to();
                let mut out = String::from_utf8_lossy(&bytes[..valid]).to_string();
                let rest = &bytes[valid..];
                // Keep a plausible prefix of a multi-byte sequence (max 3 bytes) for the next chunk.
                if rest.len() < 4 && err.error_len().is_none() {
                    self.carry = rest.to_vec();
                } else {
                    out.push_str(&String::from_utf8_lossy(rest));
                }
                out
            }
        }
    }
}

struct Session {
    info: TerminalInfo,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    buffer: Mutex<OutputBuffer>,
    alive: AtomicBool,
}

impl Session {
    fn snapshot(&self, cursor: u64) -> (Vec<u8>, u64) {
        let buffer = self.buffer.lock().expect("terminal buffer poisoned");
        buffer.read_from(cursor)
    }

    fn info(&self) -> TerminalInfo {
        let buffer = self.buffer.lock().expect("terminal buffer poisoned");
        TerminalInfo {
            alive: self.alive.load(Ordering::Relaxed),
            base: buffer.base,
            cursor: buffer.cursor(),
            ..self.info.clone()
        }
    }
}

/// Registry of live shells, shared by the web handlers and the agent runtime.
pub struct TerminalHub {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl Default for TerminalHub {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalHub {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Spawn a shell in `cwd`. `shell` defaults to `$SHELL` (then `sh`).
    pub fn open(
        &self,
        cwd: &Path,
        shell: Option<&str>,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> Result<TerminalInfo> {
        if !cwd.is_dir() {
            bail!("terminal cwd is not a directory: {}", cwd.display());
        }
        let live = self.live_count();
        if live >= MAX_SESSIONS {
            bail!("too many terminal sessions ({live} open); close one first");
        }

        let rows = clamp_dimension(rows.unwrap_or(DEFAULT_ROWS));
        let cols = clamp_dimension(cols.unwrap_or(DEFAULT_COLS));
        let shell = shell
            .map(str::to_string)
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| "sh".to_string());

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to allocate a pty")?;

        let mut command = CommandBuilder::new(&shell);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        let child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("failed to start {shell}"))?;
        // Dropping the slave side is what lets the reader see EOF once the shell exits.
        drop(pair.slave);

        let master = pair.master;
        let mut reader = master
            .try_clone_reader()
            .context("pty reader unavailable")?;
        let writer = master.take_writer().context("pty writer unavailable")?;

        let id = format!("term-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let session = Arc::new(Session {
            info: TerminalInfo {
                id: id.clone(),
                shell,
                cwd: cwd.display().to_string(),
                rows,
                cols,
                alive: true,
                base: 0,
                cursor: 0,
            },
            master: Mutex::new(master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            buffer: Mutex::new(OutputBuffer::default()),
            alive: AtomicBool::new(true),
        });

        {
            let mut sessions = self.sessions.lock().expect("terminal sessions poisoned");
            sessions.insert(id.clone(), Arc::clone(&session));
        }

        // Reader thread: the PTY is blocking I/O, so it lives off the async runtime.
        let reader_session = Arc::clone(&session);
        let thread = std::thread::Builder::new()
            .name(format!("pty-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            reader_session
                                .buffer
                                .lock()
                                .expect("terminal buffer poisoned")
                                .push(&buf[..n]);
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                reader_session.alive.store(false, Ordering::Relaxed);
            });
        match thread {
            Ok(_) => {}
            Err(e) => {
                // Without a reader the shell would look alive but never answer: fail loudly.
                let _ = self.close(&id);
                return Err(e).context("failed to spawn the pty reader thread");
            }
        }

        Ok(session.info())
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        let sessions = self.sessions.lock().expect("terminal sessions poisoned");
        let mut out: Vec<TerminalInfo> = sessions.values().map(|s| s.info()).collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn info(&self, id: &str) -> Result<TerminalInfo> {
        Ok(self.session(id)?.info())
    }

    /// Incremental output plus the cursor to resume from.
    pub fn read(&self, id: &str, cursor: u64) -> Result<(Vec<u8>, u64, bool)> {
        let session = self.session(id)?;
        let (bytes, next) = session.snapshot(cursor);
        Ok((bytes, next, session.alive.load(Ordering::Relaxed)))
    }

    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let session = self.session(id)?;
        if !session.alive.load(Ordering::Relaxed) {
            bail!("terminal {id} has exited");
        }
        let mut writer = session.writer.lock().expect("terminal writer poisoned");
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        let session = self.session(id)?;
        let size = PtySize {
            rows: clamp_dimension(rows),
            cols: clamp_dimension(cols),
            pixel_width: 0,
            pixel_height: 0,
        };
        let master = session.master.lock().expect("terminal master poisoned");
        master.resize(size)?;
        Ok(())
    }

    /// Kill the shell and forget the session. Idempotent.
    pub fn close(&self, id: &str) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.lock().expect("terminal sessions poisoned");
            sessions.remove(id)
        };
        let Some(session) = session else {
            return Ok(());
        };
        session.alive.store(false, Ordering::Relaxed);
        {
            let mut child = session.child.lock().expect("terminal child poisoned");
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn close_all(&self) {
        let ids: Vec<String> = {
            let sessions = self.sessions.lock().expect("terminal sessions poisoned");
            sessions.keys().cloned().collect()
        };
        for id in ids {
            let _ = self.close(&id);
        }
    }

    pub fn live_count(&self) -> usize {
        let sessions = self.sessions.lock().expect("terminal sessions poisoned");
        sessions
            .values()
            .filter(|s| s.alive.load(Ordering::Relaxed))
            .count()
    }

    fn session(&self, id: &str) -> Result<Arc<Session>> {
        let sessions = self.sessions.lock().expect("terminal sessions poisoned");
        sessions
            .get(id)
            .cloned()
            .with_context(|| format!("unknown terminal session: {id}"))
    }
}

impl Drop for TerminalHub {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Clamp to something a real terminal can use (0 rows would make the shell misbehave).
fn clamp_dimension(value: u16) -> u16 {
    value.clamp(2, MAX_DIMENSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_reports_cursors_and_drops_the_oldest_bytes() {
        let mut buffer = OutputBuffer::default();
        buffer.push(b"hello");
        assert_eq!(buffer.cursor(), 5);
        assert_eq!(buffer.read_from(0).0, b"hello".to_vec());
        assert_eq!(buffer.read_from(3).0, b"lo".to_vec());
        assert_eq!(buffer.read_from(99).0, Vec::<u8>::new());

        // A cursor behind the ring snaps forward instead of returning stale offsets.
        buffer.base = 10;
        assert_eq!(buffer.read_from(0).1, 15);
    }

    #[test]
    fn oversized_chunk_keeps_only_the_tail() {
        let mut buffer = OutputBuffer::default();
        let chunk = vec![b'x'; MAX_BUFFER_BYTES + 10];
        buffer.push(&chunk);
        assert_eq!(buffer.data.len(), MAX_BUFFER_BYTES);
        assert_eq!(buffer.base, 10);
        assert_eq!(buffer.cursor(), 10 + MAX_BUFFER_BYTES as u64);
    }

    #[test]
    fn utf8_chunker_holds_back_split_characters() {
        let mut chunker = Utf8Chunker::default();
        // "中" is e4 b8 ad: deliver it in two pieces across chunks.
        assert_eq!(chunker.push(&[0xe4, 0xb8]), "");
        assert_eq!(chunker.push(&[0xad, b'!']), "中!");
        assert_eq!(chunker.push(b"plain"), "plain");
        // Invalid bytes are replaced rather than swallowed.
        assert!(chunker.push(&[0xff]).contains('\u{fffd}'));
    }

    #[cfg(unix)]
    #[test]
    fn opens_a_shell_writes_and_reads_output() {
        let hub = TerminalHub::new();
        let dir = tempfile::tempdir().unwrap();
        let info = hub
            .open(dir.path(), Some("/bin/sh"), Some(24), Some(80))
            .unwrap();
        assert_eq!(info.id, "term-1");
        assert_eq!(hub.live_count(), 1);

        hub.write(&info.id, "echo synbot-pty-ok\n").unwrap();
        let mut seen = String::new();
        for _ in 0..200 {
            let (bytes, cursor, _) = hub.read(&info.id, 0).unwrap();
            seen = String::from_utf8_lossy(&bytes).to_string();
            if seen.contains("synbot-pty-ok") {
                assert!(cursor > 0);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen.contains("synbot-pty-ok"), "output was {seen:?}");

        hub.close(&info.id).unwrap();
        assert_eq!(hub.live_count(), 0);
        assert!(hub.info(&info.id).is_err());
        // Closing twice is not an error: teardown must be idempotent.
        hub.close(&info.id).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_unknown_sessions_and_bad_cwd() {
        let hub = TerminalHub::new();
        assert!(hub.write("nope", "x").is_err());
        assert!(hub.resize("nope", 10, 10).is_err());
        assert!(hub.read("nope", 0).is_err());

        let missing = tempfile::tempdir().unwrap().path().join("gone");
        assert!(hub.open(&missing, Some("/bin/sh"), None, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn session_cap_is_enforced() {
        let hub = TerminalHub::new();
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..MAX_SESSIONS {
            hub.open(dir.path(), Some("/bin/sh"), Some(10), Some(40))
                .unwrap();
        }
        let err = hub
            .open(dir.path(), Some("/bin/sh"), Some(10), Some(40))
            .unwrap_err();
        assert!(err.to_string().contains("too many terminal sessions"));
        hub.close_all();
        assert_eq!(hub.live_count(), 0);
    }
}
