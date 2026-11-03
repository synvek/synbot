import { Check, Focus, Globe, LogOut, Monitor, Moon, Sun } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { apiClient } from '../../api'
import { useTheme, type ThemeMode } from '../../contexts/ThemeContext'
import { useI18n } from '../../i18n/I18nContext'
import { MenuItem, MenuLabel, MenuSeparator, Popover } from '../../ui'
import { useShell } from './ShellContext'

const MODE_ICON: Record<ThemeMode, React.ReactNode> = {
  light: <Sun className="h-3.5 w-3.5" />,
  dark: <Moon className="h-3.5 w-3.5" />,
  system: <Monitor className="h-3.5 w-3.5" />,
}

const MODES: ThemeMode[] = ['light', 'dark', 'system']

interface AccountMenuProps {
  /** Icon-only sidebar: the menu opens above an avatar instead of above a labelled row. */
  compact?: boolean
}

/**
 * The bottom-left corner of the app: who you are, how the console looks, what language it speaks,
 * and the way out. Everything here is set once and never needed again, which is why it no longer
 * occupies a bar across the top of every screen.
 */
export function AccountMenu({ compact = false }: AccountMenuProps) {
  const { t, language, setLanguage, availableLanguages } = useI18n()
  const { mode, resolvedMode, setMode, colorScheme, setColorScheme, availableSchemes } = useTheme()
  const { toggleFocusMode } = useShell()
  const navigate = useNavigate()

  const logout = () => {
    apiClient.clearAuth()
    navigate('/login')
  }

  return (
    <Popover
      label={t('shell.account')}
      align="start"
      side="top"
      className="w-full"
      triggerClassName={compact ? undefined : 'w-full justify-start'}
      icon={
        <span
          aria-hidden="true"
          className="grid h-5 w-5 shrink-0 place-items-center rounded-[6px] bg-ink font-mono text-[10px] font-semibold leading-none text-background"
        >
          S
        </span>
      }
      text={
        compact ? undefined : (
          <span className="min-w-0 flex-1 truncate text-left">web_admin</span>
        )
      }
    >
      <MenuLabel>{t('shell.signedInAs', { name: 'web_admin' })}</MenuLabel>

      <MenuLabel>{t('shell.appearance')}</MenuLabel>
      {MODES.map((value) => (
        <MenuItem
          key={value}
          icon={MODE_ICON[value]}
          selected={mode === value}
          onSelect={() => setMode(value)}
          aside={mode === value ? <Check className="h-3.5 w-3.5 text-accent" /> : undefined}
        >
          {t(`shell.${value}`)}
        </MenuItem>
      ))}

      <MenuSeparator />
      <MenuLabel>{t('shell.accent')}</MenuLabel>
      {availableSchemes.map((scheme) => (
        <MenuItem
          key={scheme.id}
          selected={scheme.id === colorScheme.id}
          onSelect={() => setColorScheme(scheme.id)}
          icon={
            <span
              aria-hidden="true"
              className="block h-3 w-3 rounded-full ring-1 ring-inset ring-black/10"
              style={{ backgroundColor: scheme.accent[resolvedMode] }}
            />
          }
          aside={scheme.id === colorScheme.id ? <Check className="h-3.5 w-3.5 text-accent" /> : undefined}
        >
          {t(scheme.labelKey, scheme.name)}
        </MenuItem>
      ))}

      <MenuSeparator />
      <MenuLabel>
        <span className="inline-flex items-center gap-1.5">
          <Globe className="h-3 w-3" aria-hidden="true" />
          {t('common.language', 'Language')}
        </span>
      </MenuLabel>
      {availableLanguages.map((lang) => (
        <MenuItem
          key={lang.code}
          selected={language === lang.code}
          onSelect={() => setLanguage(lang.code)}
          aside={language === lang.code ? <Check className="h-3.5 w-3.5 text-accent" /> : undefined}
        >
          {lang.name}
        </MenuItem>
      ))}

      <MenuSeparator />
      <MenuItem icon={<Focus className="h-3.5 w-3.5" />} onSelect={toggleFocusMode} aside={<span className="font-mono text-micro text-ink-3">⌘.</span>}>
        {t('shell.focusMode')}
      </MenuItem>
      <MenuItem danger icon={<LogOut className="h-3.5 w-3.5" />} onSelect={logout}>
        {t('common.logout')}
      </MenuItem>
    </Popover>
  )
}
