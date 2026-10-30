import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import type { ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { RotateCcw, X } from 'lucide-react';
import { useI18n } from '../i18n/I18nContext';
import { apiClient } from '../api';
import { useTheme } from '../contexts/ThemeContext';
import type { ColorScheme } from '../contexts/ThemeContext';

interface TerminalPanelProps {
  /** Folder the shell starts in (`null` = the default workspace). */
  projectId: string | null;
  /** The tab is visible; hidden panels skip fitting so the grid is not measured at 0×0. */
  active: boolean;
  onClose: () => void;
}

type Status = 'connecting' | 'ready' | 'exited' | 'error';

/** ANSI palettes: xterm needs concrete colors, so each theme mode ships its own readable set. */
const DARK_ANSI: ITheme = {
  black: '#161b22',
  red: '#ff7b72',
  green: '#7ee787',
  yellow: '#d29922',
  blue: '#79c0ff',
  magenta: '#d2a8ff',
  cyan: '#a5d6ff',
  white: '#c9d1d9',
  brightBlack: '#6e7681',
  brightRed: '#ffa198',
  brightGreen: '#56d364',
  brightYellow: '#e3b341',
  brightBlue: '#a5d6ff',
  brightMagenta: '#e2c5ff',
  brightCyan: '#b3e0ff',
  brightWhite: '#f0f6fc',
};

const LIGHT_ANSI: ITheme = {
  black: '#24292f',
  red: '#cf222e',
  green: '#116329',
  yellow: '#7d4e00',
  blue: '#0550ae',
  magenta: '#8250df',
  cyan: '#1b7c83',
  white: '#6e7781',
  brightBlack: '#57606a',
  brightRed: '#a40e26',
  brightGreen: '#1a7f37',
  brightYellow: '#633c01',
  brightBlue: '#0969da',
  brightMagenta: '#8250df',
  brightCyan: '#1b7c83',
  brightWhite: '#8c959f',
};

/** `#rrggbb` -> `rgba(…, alpha)` so the selection tint follows the active scheme. */
function withAlpha(hex: string, alpha: number): string {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return hex;
  const value = Number.parseInt(match[1], 16);
  return `rgba(${(value >> 16) & 0xff}, ${(value >> 8) & 0xff}, ${value & 0xff}, ${alpha})`;
}

/** Terminal colors mirror the surrounding surface instead of a fixed dark palette. */
function terminalTheme(isDark: boolean, colors: ColorScheme['colors']): ITheme {
  return {
    ...(isDark ? DARK_ANSI : LIGHT_ANSI),
    background: colors.surface,
    foreground: colors.text,
    cursor: colors.primary,
    cursorAccent: colors.surface,
    selectionBackground: withAlpha(colors.primary, isDark ? 0.4 : 0.25),
  };
}

/**
 * A persistent shell in the workbench.
 *
 * The socket owns one PTY session: it is spawned on open and killed when the socket closes, so
 * switching chat tabs (component stays mounted, only hidden) keeps state — `vim` or a dev server
 * survive the switch — while leaving the page reaps the process.
 */
export default function TerminalPanel({ projectId, active, onClose }: TerminalPanelProps) {
  const { t } = useI18n();
  const { mode, colorScheme } = useTheme();
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const [status, setStatus] = useState<Status>('connecting');
  const [message, setMessage] = useState<string | null>(null);
  const [reopenToken, setReopenToken] = useState(0);

  const theme = useMemo(
    () => terminalTheme(mode === 'dark', colorScheme.colors),
    [mode, colorScheme]
  );
  // The xterm instance must survive theme switches (it owns the scrollback), so the created
  // terminal reads the theme through a ref and later changes are applied in place.
  const themeRef = useRef(theme);

  useEffect(() => {
    themeRef.current = theme;
    if (termRef.current) termRef.current.options.theme = theme;
  }, [theme]);

  const sendResize = useCallback(() => {
    const fit = fitRef.current;
    const term = termRef.current;
    const ws = wsRef.current;
    if (!fit || !term || !ws || ws.readyState !== WebSocket.OPEN) return;
    try {
      fit.fit();
    } catch {
      // Container hidden or not laid out yet: keep the current grid.
    }
    ws.send(JSON.stringify({ type: 'resize', rows: term.rows, cols: term.cols }));
  }, []);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
      fontSize: 13,
      fontWeight: '400',
      fontWeightBold: '600',
      lineHeight: 1.35,
      letterSpacing: 0,
      cursorBlink: true,
      scrollback: 5000,
      theme: themeRef.current,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    termRef.current = term;
    fitRef.current = fit;
    try {
      fit.fit();
    } catch {
      // ignore: fitted again on the first resize/visibility change
    }

    // Same-origin on the web; absolute loopback URL plus the desktop token in the app.
    const ws = new WebSocket(apiClient.wsUrl('/ws/terminal'));
    wsRef.current = ws;
    setStatus('connecting');
    setMessage(null);

    ws.onopen = () => {
      ws.send(
        JSON.stringify({
          type: 'open',
          project_id: projectId,
          rows: term.rows,
          cols: term.cols,
        })
      );
    };

    ws.onmessage = (event) => {
      let frame: { type?: string; data?: string; message?: string; shell?: string; cwd?: string };
      try {
        frame = JSON.parse(String(event.data));
      } catch {
        return;
      }
      if (frame.type === 'opened') {
        setStatus('ready');
        term.write(`\r\n\x1b[2m# ${frame.shell ?? 'shell'} — ${frame.cwd ?? ''}\x1b[0m\r\n`);
      } else if (frame.type === 'output' && frame.data) {
        term.write(frame.data);
      } else if (frame.type === 'exit') {
        setStatus('exited');
        term.write('\r\n\x1b[2m[process exited]\x1b[0m\r\n');
      } else if (frame.type === 'error') {
        setStatus('error');
        setMessage(frame.message ?? 'terminal error');
      }
    };

    ws.onerror = () => {
      setStatus('error');
      setMessage('terminal connection failed');
    };
    ws.onclose = () => {
      setStatus((current) => (current === 'exited' ? current : 'error'));
    };

    const dataSub = term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'input', data }));
      }
    });

    const observer = new ResizeObserver(() => {
      if (host.clientWidth < 2 || host.clientHeight < 2) return;
      sendResize();
    });
    observer.observe(host);

    return () => {
      observer.disconnect();
      dataSub.dispose();
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'close' }));
      }
      ws.close();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      wsRef.current = null;
    };
  }, [projectId, reopenToken, sendResize]);

  // Fit again when the tab becomes visible (the panel is hidden, not unmounted, while chatting).
  useEffect(() => {
    if (!active) return;
    const id = window.requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
        termRef.current?.focus();
      } catch {
        // ignore
      }
    });
    return () => window.cancelAnimationFrame(id);
  }, [active]);

  const statusLabel = t(`terminal.status.${status}`, status === 'ready' ? 'connected' : status);

  return (
    <div className="flex flex-col h-full min-h-0 bg-surface">
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border text-xs text-text-secondary shrink-0">
        <span
          className={`w-2 h-2 rounded-full ${
            status === 'ready' ? 'bg-success' : status === 'error' ? 'bg-error' : 'bg-text-secondary'
          }`}
        />
        <span>{statusLabel}</span>
        {message && <span className="text-error truncate">{message}</span>}
        <button
          type="button"
          onClick={() => setReopenToken((value) => value + 1)}
          className="ml-auto p-1 rounded hover:text-text"
          title={t('terminal.restart', '重启终端')}
        >
          <RotateCcw size={12} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="p-1 rounded hover:text-text"
          title={t('terminal.close', '关闭终端')}
        >
          <X size={12} />
        </button>
      </div>
      <div ref={hostRef} className="flex-1 min-h-0 p-1" />
    </div>
  );
}
