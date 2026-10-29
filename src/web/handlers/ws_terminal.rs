//! `/ws/terminal` — the workbench shell.
//!
//! A thin WebSocket bridge over [`crate::terminal::TerminalHub`]:
//!
//! Client → server (JSON text frames)
//! - `{"type":"open","project_id":null,"rows":24,"cols":80}` — spawn a shell in the chat's workspace
//! - `{"type":"input","data":"ls\r"}` — bytes typed in xterm
//! - `{"type":"resize","rows":30,"cols":100}`
//! - `{"type":"close"}` — kill the shell (closing the socket does the same)
//! - `{"type":"ping"}`
//!
//! Server → client
//! - `{"type":"opened","id":"term-1","cwd":"/path","shell":"/bin/zsh","rows":24,"cols":80}`
//! - `{"type":"output","data":"..."}` — always valid UTF-8; split characters are carried over
//! - `{"type":"exit"}` — the shell ended (session is gone; send `open` to start another)
//! - `{"type":"error","message":"..."}` / `{"type":"pong"}`
//!
//! Output is polled from the hub's ring buffer on a fixed tick instead of being pushed from the
//! reader thread: the actor then needs no cross-thread `WebsocketContext`, and a slow client simply
//! reads more bytes on the next tick — no loss, no duplication, because reads are cursor-based.

use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Handler, Message, StreamHandler, WrapFuture,
};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::terminal::{TerminalHub, Utf8Chunker};
use crate::web::state::AppState;

/// How often buffered shell output is flushed to the socket.
const FLUSH_INTERVAL: Duration = Duration::from_millis(30);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalClientMessage {
    Open {
        #[serde(default)]
        project_id: Option<String>,
        #[serde(default)]
        rows: Option<u16>,
        #[serde(default)]
        cols: Option<u16>,
    },
    Input {
        data: String,
    },
    Resize {
        rows: u16,
        cols: u16,
    },
    Close,
    Ping,
}

/// Result of the async workspace lookup + spawn, delivered back to the actor.
#[derive(Message)]
#[rtype(result = "()")]
struct Opened {
    result: Result<crate::terminal::TerminalInfo, String>,
}

pub struct TerminalWs {
    state: AppState,
    hub: Arc<TerminalHub>,
    session_id: Option<String>,
    cursor: u64,
    chunker: Utf8Chunker,
    exit_sent: bool,
    heartbeat: Instant,
}

impl TerminalWs {
    pub fn new(state: AppState) -> Self {
        let hub = Arc::clone(&state.terminals);
        Self {
            state,
            hub,
            session_id: None,
            cursor: 0,
            chunker: Utf8Chunker::default(),
            exit_sent: false,
            heartbeat: Instant::now(),
        }
    }

    fn open(
        &mut self,
        project_id: Option<String>,
        rows: Option<u16>,
        cols: Option<u16>,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        if self.session_id.is_some() {
            // One shell per socket keeps the protocol unambiguous; the UI opens a new socket per tab.
            self.send_error(ctx, "a terminal is already open on this connection");
            return;
        }
        let state = self.state.clone();
        let hub = Arc::clone(&self.hub);
        let addr = ctx.address();
        ctx.spawn(
            async move {
                let root = resolve_workspace(&state, project_id.as_deref()).await;
                let result = match root {
                    Ok(root) => hub
                        .open(&root, None, rows, cols)
                        .map_err(|e| format!("{e:#}")),
                    Err(message) => Err(message),
                };
                addr.do_send(Opened { result });
            }
            .into_actor(self),
        );
    }

    fn flush_output(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        let Some(id) = self.session_id.clone() else {
            return;
        };
        match self.hub.read(&id, self.cursor) {
            Ok((bytes, next, alive)) => {
                if next != self.cursor {
                    self.cursor = next;
                    let text = self.chunker.push(&bytes);
                    if !text.is_empty() {
                        ctx.text(json!({ "type": "output", "data": text }).to_string());
                    }
                }
                if !alive && !self.exit_sent {
                    self.exit_sent = true;
                    self.session_id = None;
                    ctx.text(json!({ "type": "exit" }).to_string());
                    let _ = self.hub.close(&id);
                }
            }
            Err(e) => {
                self.send_error(ctx, &format!("terminal read failed: {e:#}"));
                self.session_id = None;
            }
        }
    }

    fn write_input(&mut self, data: &str, ctx: &mut ws::WebsocketContext<Self>) {
        let Some(id) = self.session_id.clone() else {
            self.send_error(ctx, "no terminal is open");
            return;
        };
        if let Err(e) = self.hub.write(&id, data) {
            self.send_error(ctx, &format!("{e:#}"));
        }
    }

    fn resize(&mut self, rows: u16, cols: u16, ctx: &mut ws::WebsocketContext<Self>) {
        let Some(id) = self.session_id.clone() else {
            return;
        };
        if let Err(e) = self.hub.resize(&id, rows, cols) {
            self.send_error(ctx, &format!("{e:#}"));
        }
    }

    fn close_session(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(id) = self.session_id.take() {
            let _ = self.hub.close(&id);
            ctx.text(json!({ "type": "exit" }).to_string());
        }
    }

    fn send_error(&self, ctx: &mut ws::WebsocketContext<Self>, message: &str) {
        ctx.text(json!({ "type": "error", "message": message }).to_string());
    }
}

impl Handler<Opened> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: Opened, ctx: &mut Self::Context) {
        match msg.result {
            Ok(info) => {
                self.session_id = Some(info.id.clone());
                self.cursor = 0;
                self.exit_sent = false;
                self.chunker = Utf8Chunker::default();
                ctx.text(
                    json!({
                        "type": "opened",
                        "id": info.id,
                        "cwd": info.cwd,
                        "shell": info.shell,
                        "rows": info.rows,
                        "cols": info.cols,
                    })
                    .to_string(),
                );
            }
            Err(message) => self.send_error(ctx, &message),
        }
    }
}

impl Actor for TerminalWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(FLUSH_INTERVAL, |act, ctx| act.flush_output(ctx));
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                tracing::info!("terminal websocket timed out");
                act.close_session(ctx);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        // Closing the socket reaps the shell: sessions only live as long as a panel shows them.
        if let Some(id) = self.session_id.take() {
            let _ = self.hub.close(&id);
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for TerminalWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(payload)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&payload);
            }
            Ok(ws::Message::Pong(_)) => self.heartbeat = Instant::now(),
            Ok(ws::Message::Text(text)) => {
                self.heartbeat = Instant::now();
                match serde_json::from_str::<TerminalClientMessage>(&text) {
                    Ok(TerminalClientMessage::Open {
                        project_id,
                        rows,
                        cols,
                    }) => self.open(project_id, rows, cols, ctx),
                    Ok(TerminalClientMessage::Input { data }) => self.write_input(&data, ctx),
                    Ok(TerminalClientMessage::Resize { rows, cols }) => {
                        self.resize(rows, cols, ctx)
                    }
                    Ok(TerminalClientMessage::Close) => self.close_session(ctx),
                    Ok(TerminalClientMessage::Ping) => {
                        ctx.text(json!({ "type": "pong" }).to_string())
                    }
                    Err(e) => self.send_error(ctx, &format!("invalid message: {e}")),
                }
            }
            Ok(ws::Message::Close(reason)) => {
                self.close_session(ctx);
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Binary(_)) => self.send_error(ctx, "binary frames are not supported"),
            Err(e) => {
                tracing::warn!(error = %e, "terminal websocket protocol error");
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// The folder the shell starts in: the chat's project, else the configured workspace.
async fn resolve_workspace(state: &AppState, project_id: Option<&str>) -> Result<PathBuf, String> {
    if let Some(id) = project_id {
        let store = state.projects.read().await;
        let project = store
            .get(id)
            .ok_or_else(|| format!("project not found: {id}"))?;
        return Ok(project.path.clone());
    }
    let config = state.config.read().await;
    Ok(crate::config::normalize_workspace_path(
        std::path::Path::new(&config.main_agent.workspace),
    ))
}

pub async fn ws_terminal(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    ws::start(TerminalWs::new(state.get_ref().clone()), &req, stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_with_and_without_project() {
        let open: TerminalClientMessage =
            serde_json::from_str(r#"{"type":"open","rows":30,"cols":100}"#).unwrap();
        match open {
            TerminalClientMessage::Open {
                project_id,
                rows,
                cols,
            } => {
                assert!(project_id.is_none());
                assert_eq!(rows, Some(30));
                assert_eq!(cols, Some(100));
            }
            other => panic!("unexpected: {other:?}"),
        }
        let with_project: TerminalClientMessage =
            serde_json::from_str(r#"{"type":"open","project_id":"p1"}"#).unwrap();
        match with_project {
            TerminalClientMessage::Open { project_id, .. } => {
                assert_eq!(project_id.as_deref(), Some("p1"))
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_input_resize_and_rejects_unknown_types() {
        let input: TerminalClientMessage =
            serde_json::from_str(r#"{"type":"input","data":"ls\r"}"#).unwrap();
        assert!(matches!(input, TerminalClientMessage::Input { .. }));

        let resize: TerminalClientMessage =
            serde_json::from_str(r#"{"type":"resize","rows":40,"cols":120}"#).unwrap();
        match resize {
            TerminalClientMessage::Resize { rows, cols } => {
                assert_eq!((rows, cols), (40, 120));
            }
            other => panic!("unexpected: {other:?}"),
        }

        assert!(serde_json::from_str::<TerminalClientMessage>(r#"{"type":"shell"}"#).is_err());
    }
}
