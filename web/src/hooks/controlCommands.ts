/**
 * Built-in control commands.
 *
 * Mirrors `src/agent/control_commands.rs` (plus the `/workflow` trigger). They are answered by the
 * agent loop itself and never start a turn, so no `turn_started`/`turn_finished` pair brackets
 * them: an optimistic "running" state would never be cleared and the composer would stay disabled
 * for good — `/clear` is the classic case, because the daemon is already idle when it replies.
 */
const CONTROL_COMMAND =
  /^\/((stop|cancel|resume|status|clear|commands|help|skills|tools|pending|approvals|tasks|todo)\s*|(approve|reject|workflow)(\s[\s\S]*)?)$/i;

/** Commands that end work rather than start it. */
const ENDS_TURN = /^\/(stop|cancel)\s*$/i;

/** True when the daemon answers this message itself, so no turn events follow. */
export function isControlCommand(content: string): boolean {
  return CONTROL_COMMAND.test(content.trim());
}

/** True when the command should drop the local "a turn is in flight" state. */
export function endsTurn(content: string): boolean {
  return ENDS_TURN.test(content.trim());
}
