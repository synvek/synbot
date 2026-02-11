import React from 'react'
import { useI18n } from '../I18nContext'
import { Globe } from 'lucide-react'

const LanguageSwitcher: React.FC = () => {
  const { language, setLanguage, availableLanguages } = useI18n()

  return (
    <div className="relative group">
      <button
        className="flex items-center space-x-2 px-3 py-2 text-sm text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
        title={`Current language: ${availableLanguages.find(l => l.code === language)?.name}`}
      >
        <Globe className="w-4 h-4" />
        <span>{language.toUpperCase()}</span>
      </button>
      
      <div className="absolute right-0 mt-2 w-32 bg-surface border border-border rounded-lg shadow-lg z-50 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-200">
        <div className="py-1">
          {availableLanguages.map((lang) => (
            <button
              key={lang.code}
              onClick={() => setLanguage(lang.code)}
              className={`w-full text-left px-4 py-2.5 text-sm transition-all ${
                language === lang.code
                  ? 'bg-primary text-white font-medium'
                  : 'text-text hover:bg-background'
              }`}
            >
              {lang.name}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

export default LanguageSwitcher