import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { Focus, LogOut } from 'lucide-react'
import { apiClient } from '../../api'
import { useTheme } from '../../contexts/ThemeContext'
import { useI18n } from '../../i18n/I18nContext'
import { CommandPalette, type CommandPaletteItem } from '../../ui'
import { navItems } from './nav'
import { useShell } from './ShellContext'

/**
 * ⌘K. It reaches everything the console can open without loading a page: the twelve routes, the
 * six accents, the three appearance modes, focus mode, and sign-out.
 */
export function CommandPaletteHost() {
  const navigate = useNavigate()
  const { t } = useI18n()
  const { paletteOpen, setPaletteOpen, toggleFocusMode } = useShell()
  const { setColorScheme, setMode, colorScheme, availableSchemes } = useTheme()

  const items = useMemo<CommandPaletteItem[]>(() => {
    const pages: CommandPaletteItem[] = navItems.map((item) => ({
      id: `page:${item.path}`,
      label: t(item.labelKey),
      group: t('shell.groups'),
      hint: item.path,
      keywords: item.path,
      run: () => navigate(item.path),
    }))

    const appearances: CommandPaletteItem[] = [
      ...(['light', 'dark', 'system'] as const).map((mode) => ({
        id: `mode:${mode}`,
        label: t(`shell.${mode}`),
        group: t('shell.appearanceGroup'),
        hint: mode === 'system' ? 'auto' : mode,
        run: () => setMode(mode),
      })),
      ...availableSchemes.map((scheme) => ({
        id: `accent:${scheme.id}`,
        label: t(scheme.labelKey, scheme.name),
        group: t('shell.appearanceGroup'),
        hint: scheme.id,
        run: () => setColorScheme(scheme.id),
      })),
    ]

    const actions: CommandPaletteItem[] = [
      {
        id: 'action:focus',
        label: t('shell.focusMode'),
        group: t('shell.actions'),
        hint: '⌘.',
        icon: <Focus className="h-3.5 w-3.5" />,
        run: toggleFocusMode,
      },
      {
        id: 'action:logout',
        label: t('common.logout'),
        group: t('shell.actions'),
        icon: <LogOut className="h-3.5 w-3.5" />,
        run: () => {
          apiClient.clearAuth()
          navigate('/login')
        },
      },
    ]

    return [...pages, ...appearances, ...actions]
  }, [availableSchemes, colorScheme.id, navigate, setColorScheme, setMode, t, toggleFocusMode])

  return (
    <CommandPalette
      open={paletteOpen}
      onClose={() => setPaletteOpen(false)}
      items={items}
      placeholder={t('shell.searchPlaceholder')}
      emptyLabel={t('shell.noResults')}
    />
  )
}
