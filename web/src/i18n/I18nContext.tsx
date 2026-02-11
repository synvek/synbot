import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'

export type Language = 'en' | 'zh'

export interface I18nContextType {
  language: Language
  setLanguage: (lang: Language) => void
  t: (key: string, params?: Record<string, string | number>) => string
  availableLanguages: { code: Language; name: string }[]
}

const I18nContext = createContext<I18nContextType | undefined>(undefined)

export const useI18n = () => {
  const context = useContext(I18nContext)
  if (!context) {
    throw new Error('useI18n must be used within an I18nProvider')
  }
  return context
}

interface I18nProviderProps {
  children: ReactNode
}

export const I18nProvider: React.FC<I18nProviderProps> = ({ children }) => {
  const [language, setLanguage] = useState<Language>(() => {
    const saved = localStorage.getItem('language')
    return (saved as Language) || 'en'
  })

  const [translations, setTranslations] = useState<Record<string, any>>({})

  const availableLanguages = [
    { code: 'en' as Language, name: 'English' },
    { code: 'zh' as Language, name: '中文' }
  ]

  // Load translations
  useEffect(() => {
    const loadTranslations = async () => {
      try {
        const module = await import(`./locales/${language}.json`)
        setTranslations(module.default)
      } catch (error) {
        console.error(`Failed to load translations for ${language}:`, error)
        // Fallback to English
        try {
          const module = await import('./locales/en.json')
          setTranslations(module.default)
        } catch (fallbackError) {
          console.error('Failed to load fallback translations:', fallbackError)
        }
      }
    }

    loadTranslations()
  }, [language])

  const handleSetLanguage = (lang: Language) => {
    setLanguage(lang)
    localStorage.setItem('language', lang)
  }

  const t = (key: string, params?: Record<string, string | number>): string => {
    const keys = key.split('.')
    let value: any = translations
    
    for (const k of keys) {
      if (value && typeof value === 'object' && k in value) {
        value = value[k]
      } else {
        // Return the key if translation not found
        return key
      }
    }

    if (typeof value !== 'string') {
      return key
    }

    // Replace parameters if provided
    if (params) {
      return Object.entries(params).reduce((result, [paramKey, paramValue]) => {
        return result.replace(`{{${paramKey}}}`, String(paramValue))
      }, value)
    }

    return value
  }

  return (
    <I18nContext.Provider
      value={{
        language,
        setLanguage: handleSetLanguage,
        t,
        availableLanguages
      }}
    >
      {children}
    </I18nContext.Provider>
  )
}