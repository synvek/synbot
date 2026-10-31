import { Check, Globe } from 'lucide-react'
import { useI18n } from '../I18nContext'
import { MenuItem, MenuLabel, Popover } from '../../ui'

/**
 * Language switch. It used to open on hover; a menu that appears when the pointer crosses it is
 * impossible to use on touch and easy to lose on the way to the item, so it is a click menu now.
 */
const LanguageSwitcher: React.FC = () => {
  const { language, setLanguage, availableLanguages, t } = useI18n()

  return (
    <Popover label={t('common.language', 'Language')} icon={<Globe className="h-4 w-4" />}>
      <MenuLabel>{t('common.language', 'Language')}</MenuLabel>
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
    </Popover>
  )
}

export default LanguageSwitcher
