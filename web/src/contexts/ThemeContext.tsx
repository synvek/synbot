import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'

/**
 * The console keeps one neutral scale in `styles/tokens.css`; a colour scheme is an accent ramp
 * applied over it through the `data-accent` attribute on <html>. Nothing else changes with the
 * scheme — that is what keeps twelve pages coherent in six colours and two modes.
 */
export type ThemeMode = 'light' | 'dark' | 'system'
export type ResolvedMode = 'light' | 'dark'

export type ColorScheme = {
  id: string
  /** zh label, kept for callers that do not go through i18n. */
  name: string
  /** i18n key for the visible label. */
  labelKey: string
  accent: { light: string; dark: string }
  /**
   * Concrete colours resolved for the current mode. xterm and a few inline styles need real
   * values rather than CSS variables; everything else reads the variables.
   */
  colors: {
    primary: string
    secondary: string
    accent: string
    background: string
    surface: string
    text: string
    textSecondary: string
    border: string
    success: string
    warning: string
    error: string
    info: string
  }
}

const LIGHT_NEUTRALS = {
  background: '#ffffff',
  surface: '#ffffff',
  text: '#17181b',
  textSecondary: '#5c5f66',
  border: '#e7e7ea',
  success: '#10875b',
  warning: '#b4690e',
  error: '#cf3b32',
  info: '#2f6fed',
}

const DARK_NEUTRALS = {
  background: '#0b0c0e',
  surface: '#131417',
  text: '#e9eaec',
  textSecondary: '#a2a6ae',
  border: '#242629',
  success: '#35a97c',
  warning: '#d18a2b',
  error: '#e5645b',
  info: '#5b8dff',
}

type SchemeDefinition = Pick<ColorScheme, 'id' | 'name' | 'labelKey' | 'accent'>

/** The six shipped schemes. Ids are part of the product (stored in `theme-scheme`). */
export const accentSchemes: SchemeDefinition[] = [
  {
    id: 'blue-ocean',
    name: '蓝色海洋',
    labelKey: 'colorSchemes.blueOcean',
    accent: { light: '#2f6fed', dark: '#5b8dff' },
  },
  {
    id: 'green-forest',
    name: '绿色森林',
    labelKey: 'colorSchemes.greenForest',
    accent: { light: '#12805c', dark: '#2aa37b' },
  },
  {
    id: 'purple-mystic',
    name: '紫色神秘',
    labelKey: 'colorSchemes.purpleMystic',
    accent: { light: '#7c5cfc', dark: '#9b7fff' },
  },
  {
    id: 'orange-sunset',
    name: '橙色日落',
    labelKey: 'colorSchemes.orangeSunset',
    accent: { light: '#d2591a', dark: '#e8733a' },
  },
  {
    id: 'pink-blossom',
    name: '粉色花语',
    labelKey: 'colorSchemes.pinkBlossom',
    accent: { light: '#db2777', dark: '#ee5c97' },
  },
  {
    id: 'gray-modern',
    name: '灰色现代',
    labelKey: 'colorSchemes.grayModern',
    accent: { light: '#3f4147', dark: '#a7abb4' },
  },
]

function schemeColors(scheme: SchemeDefinition, mode: ResolvedMode): ColorScheme['colors'] {
  const accent = scheme.accent[mode]
  return {
    ...(mode === 'dark' ? DARK_NEUTRALS : LIGHT_NEUTRALS),
    primary: accent,
    secondary: accent,
    accent,
  }
}

function readStoredMode(): ThemeMode {
  const saved = localStorage.getItem('theme-mode')
  return saved === 'dark' || saved === 'system' ? saved : 'light'
}

function readStoredScheme(): string {
  const saved = localStorage.getItem('theme-scheme')
  return accentSchemes.some((scheme) => scheme.id === saved) ? (saved as string) : 'blue-ocean'
}

function systemPrefersDark(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches
  )
}

interface ThemeContextType {
  mode: ThemeMode
  /** `mode` with `system` resolved against the OS. */
  resolvedMode: ResolvedMode
  colorScheme: ColorScheme
  setMode: (mode: ThemeMode) => void
  setColorScheme: (schemeId: string) => void
  toggleMode: () => void
  availableSchemes: ColorScheme[]
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined)

export const useTheme = () => {
  const context = useContext(ThemeContext)
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }
  return context
}

interface ThemeProviderProps {
  children: ReactNode
}

export const ThemeProvider: React.FC<ThemeProviderProps> = ({ children }) => {
  const [mode, setModeState] = useState<ThemeMode>(readStoredMode)
  const [colorSchemeId, setColorSchemeId] = useState<string>(readStoredScheme)
  const [systemDark, setSystemDark] = useState<boolean>(systemPrefersDark)

  // Follow the OS while `system` is selected, so a scheduled dark switch needs no reload.
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return
    const query = window.matchMedia('(prefers-color-scheme: dark)')
    const listener = (event: MediaQueryListEvent) => setSystemDark(event.matches)
    query.addEventListener('change', listener)
    return () => query.removeEventListener('change', listener)
  }, [])

  const resolvedMode: ResolvedMode = mode === 'system' ? (systemDark ? 'dark' : 'light') : mode

  const definition = useMemo(
    () => accentSchemes.find((scheme) => scheme.id === colorSchemeId) ?? accentSchemes[0],
    [colorSchemeId]
  )

  const colorScheme = useMemo<ColorScheme>(
    () => ({ ...definition, colors: schemeColors(definition, resolvedMode) }),
    [definition, resolvedMode]
  )

  const availableSchemes = useMemo<ColorScheme[]>(
    () =>
      accentSchemes.map((scheme) => ({
        ...scheme,
        colors: schemeColors(scheme, resolvedMode),
      })),
    [resolvedMode]
  )

  const setMode = useCallback((next: ThemeMode) => {
    setModeState(next)
    localStorage.setItem('theme-mode', next)
  }, [])

  const setColorScheme = useCallback((schemeId: string) => {
    setColorSchemeId(schemeId)
    localStorage.setItem('theme-scheme', schemeId)
  }, [])

  const toggleMode = useCallback(() => {
    setModeState((previous) => {
      const current = previous === 'system' ? (systemPrefersDark() ? 'dark' : 'light') : previous
      const next: ResolvedMode = current === 'light' ? 'dark' : 'light'
      localStorage.setItem('theme-mode', next)
      return next
    })
  }, [])

  // The accent is a single attribute; the dark class is what tokens.css keys its scale off.
  useEffect(() => {
    const root = document.documentElement
    root.setAttribute('data-accent', colorScheme.id)
    root.classList.toggle('dark', resolvedMode === 'dark')
  }, [colorScheme.id, resolvedMode])

  return (
    <ThemeContext.Provider
      value={{
        mode,
        resolvedMode,
        colorScheme,
        setMode,
        setColorScheme,
        toggleMode,
        availableSchemes,
      }}
    >
      {children}
    </ThemeContext.Provider>
  )
}
