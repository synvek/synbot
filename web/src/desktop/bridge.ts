/**
 * Bridge to the Tauri shell.
 *
 * The desktop app serves this UI from bundled assets on a custom scheme, so it cannot assume the
 * daemon lives on `window.location.host`: the shell tells it where the daemon is (`apiBase`,
 * `wsBase`) and which per-launch token to send. Everything here degrades to a no-op on the web
 * build, which keeps a single frontend for both transports.
 *
 * The shell never imports `@tauri-apps/api`: calls go through `__TAURI_INTERNALS__`, which Tauri
 * injects into every app page, and shell→page messages arrive as plain DOM events.
 */

/** Payload of the `desktop_bootstrap` command (`desktop/src-tauri/src/commands.rs`). */
export interface DesktopBootstrap {
  apiBase: string;
  wsBase: string;
  token: string;
  version: string;
  platform: string;
}

/** DOM event the tray dispatches when the user picks a folder from the tray menu. */
export const OPEN_PROJECT_EVENT = 'synbot:open-project';

interface TauriInternals {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
}

function internals(): TauriInternals | null {
  const candidate = (window as unknown as { __TAURI_INTERNALS__?: TauriInternals })
    .__TAURI_INTERNALS__;
  return candidate && typeof candidate.invoke === 'function' ? candidate : null;
}

/** True inside the Tauri shell (bundled page or the Vite dev server it opens). */
export function isDesktop(): boolean {
  return internals() !== null;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const tauri = internals();
  if (!tauri) {
    throw new Error(`IPC command '${command}' is only available in the desktop app`);
  }
  return (await tauri.invoke(command, args)) as T;
}

/**
 * Asks the shell to start (or reuse) the local daemon and hand back its address and secret.
 * Returns `null` on the web build so callers can fall back to cookie/Basic auth.
 */
export async function bootstrapDesktop(): Promise<DesktopBootstrap | null> {
  if (!isDesktop()) return null;
  return invoke<DesktopBootstrap>('desktop_bootstrap');
}

/** Native folder picker; `null` when the user cancels or when running in a browser. */
export async function pickFolder(): Promise<string | null> {
  if (!isDesktop()) return null;
  const picked = await invoke<string | null>('desktop_pick_folder');
  return picked ?? null;
}

/**
 * System notification for a finished turn. Failures are swallowed on purpose: a missing
 * notification permission must never break the chat.
 */
export async function notifyDesktop(title: string, body: string): Promise<void> {
  if (!isDesktop()) return;
  try {
    await invoke('desktop_notify', { title, body });
  } catch {
    /* notifications are best effort */
  }
}

/** Subscribes to "open this folder" requests coming from the tray. Returns an unsubscribe fn. */
export function onOpenProject(handler: (path: string) => void): () => void {
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<unknown>).detail;
    if (typeof detail === 'string' && detail.trim()) handler(detail);
  };
  window.addEventListener(OPEN_PROJECT_EVENT, listener);
  return () => window.removeEventListener(OPEN_PROJECT_EVENT, listener);
}
