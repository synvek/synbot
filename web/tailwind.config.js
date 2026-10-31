/**
 * Colours live in CSS variables so a theme switch is one attribute on <html>. Tailwind cannot read
 * an alpha channel out of a hex held in a variable, and silently emits nothing for `bg-error/10`,
 * so every token is exposed as a colour function that composes the mix itself. Variables passed
 * through untouched (`var(--tw-*-opacity)`) keep the plain colour.
 */
const token = (variable) => ({ opacityValue }) => {
  if (opacityValue === undefined || opacityValue.startsWith('var(')) {
    return `var(${variable})`;
  }
  const percent = Number(opacityValue) * 100;
  return Number.isFinite(percent)
    ? `color-mix(in srgb, var(${variable}) ${percent}%, transparent)`
    : `var(${variable})`;
};

/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Semantic layer. `surface-2` is the inset (code, hover, table header), `ink-*` is text.
        background: token('--color-background'),
        surface: token('--color-surface'),
        'surface-2': token('--color-surface-2'),
        ink: token('--color-text'),
        'ink-2': token('--color-ink-2'),
        'ink-3': token('--color-ink-3'),
        line: token('--color-line'),
        'line-strong': token('--color-line-strong'),
        accent: {
          DEFAULT: token('--color-accent'),
          hover: token('--color-accent-hover'),
          fg: token('--color-accent-fg'),
        },
        focus: token('--color-focus'),
        // Legacy aliases kept for the pages that have not migrated yet.
        primary: token('--color-primary'),
        secondary: token('--color-secondary'),
        text: token('--color-text'),
        'text-secondary': token('--color-textSecondary'),
        border: token('--color-border'),
        success: token('--color-success'),
        warning: token('--color-warning'),
        error: token('--color-error'),
        info: token('--color-info'),
      },
      fontFamily: {
        sans: [
          'Geist',
          '-apple-system',
          'BlinkMacSystemFont',
          'Segoe UI',
          'PingFang SC',
          'Hiragino Sans GB',
          'Microsoft YaHei',
          'Helvetica Neue',
          'Arial',
          'sans-serif',
        ],
        mono: [
          'Geist Mono',
          'ui-monospace',
          'SFMono-Regular',
          'SF Mono',
          'Menlo',
          'Consolas',
          'monospace',
        ],
      },
      fontSize: {
        micro: ['12px', { lineHeight: '1.45' }],
        small: ['13px', { lineHeight: '1.45' }],
        body: ['14px', { lineHeight: '1.45' }],
        section: ['16px', { lineHeight: '1.3' }],
        page: ['20px', { lineHeight: '1.25', letterSpacing: '-0.01em' }],
        display: ['26px', { lineHeight: '1.2', letterSpacing: '-0.01em' }],
      },
      borderRadius: {
        control: '6px',
        panel: '10px',
        pill: '999px',
      },
      boxShadow: {
        overlay: '0 12px 32px rgba(0, 0, 0, 0.14), 0 1px 2px rgba(0, 0, 0, 0.08)',
      },
      transitionDuration: {
        fast: '120ms',
        base: '160ms',
        overlay: '220ms',
      },
      maxWidth: {
        transcript: '760px',
      },
      keyframes: {
        'dot-breathe': {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.35' },
        },
      },
      animation: {
        'dot-breathe': 'dot-breathe 1.4s ease-in-out infinite',
      },
    },
  },
  plugins: [],
}
