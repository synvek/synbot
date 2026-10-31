import { Check, Monitor, Moon, Palette, Sun } from 'lucide-react'
import { useTheme, type ThemeMode } from '../../contexts/ThemeContext'
import { useI18n } from '../../i18n/I18nContext'
import { MenuItem, MenuLabel, MenuSeparator, Popover } from '../../ui'

const MODE_ICON: Record<ThemeMode, React.ReactNode> = {
  light: <Sun className="h-3.5 w-3.5" />,
  dark: <Moon className="h-3.5 w-3.5" />,
  system: <Monitor className="h-3.5 w-3.5" />,
}

/** Appearance menu: mode first, then the accent ramp. The swatch is the only colour in the bar. */
export function ThemeMenu() {
  const { mode, resolvedMode, setMode, colorScheme, setColorScheme, availableSchemes } = useTheme()
  const { t } = useI18n()

  const modes: ThemeMode[] = ['light', 'dark', 'system']

  return (
    <Popover label={t('shell.appearance')} icon={<Palette className="h-4 w-4" />}>
      <MenuLabel>{t('shell.appearance')}</MenuLabel>
      {modes.map((value) => (
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
    </Popover>
  )
}
