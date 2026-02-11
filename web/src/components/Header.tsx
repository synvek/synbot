import React, { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { apiClient } from '../api/client'
import { useTheme, colorSchemes, darkColorSchemes } from '../contexts/ThemeContext'
import { useI18n } from '../i18n/I18nContext'
import LanguageSwitcher from '../i18n/components/LanguageSwitcher'
import { Sun, Moon, Palette, ChevronDown } from 'lucide-react'

const Header: React.FC = () => {
  const navigate = useNavigate()
  const { mode, colorScheme, toggleMode, setColorScheme, availableSchemes } = useTheme()
  const { t } = useI18n()
  const [showColorMenu, setShowColorMenu] = useState(false)

  const handleLogout = () => {
    apiClient.clearAuth()
    navigate('/login')
  }

  const handleColorSchemeSelect = (schemeId: string) => {
    setColorScheme(schemeId)
    setShowColorMenu(false)
  }

  const getCurrentSchemeName = () => {
    const scheme = availableSchemes.find(s => s.id === colorScheme.id)
    if (!scheme) return t('colorSchemes.blueOcean')
    
    // Map scheme IDs to translation keys
    const schemeMap: Record<string, string> = {
      'blue-ocean': 'colorSchemes.blueOcean',
      'green-forest': 'colorSchemes.greenForest',
      'purple-mystic': 'colorSchemes.purpleMystic',
      'orange-sunset': 'colorSchemes.orangeSunset',
      'pink-blossom': 'colorSchemes.pinkBlossom',
      'gray-modern': 'colorSchemes.grayModern'
    }
    
    return t(schemeMap[scheme.id] || 'colorSchemes.blueOcean')
  }

  return (
    <header className="bg-surface shadow-sm border-b border-border">
      <div className="px-6 py-4 flex justify-between items-center">
        <h1 className="text-2xl font-bold text-text">
          {t('app.title')}
        </h1>
        
        <div className="flex items-center space-x-4">
          {/* 语言切换器 */}
          <LanguageSwitcher />
          
          {/* 主题切换按钮 */}
          <div className="relative">
            <button
              onClick={() => setShowColorMenu(!showColorMenu)}
              className="flex items-center space-x-1 px-3 py-2 text-sm text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
              title={getCurrentSchemeName()}
            >
              <Palette className="w-4 h-4" />
              <ChevronDown className="w-3 h-3" />
            </button>
            
            {showColorMenu && (
              <div className="absolute right-0 mt-2 w-56 bg-surface border border-border rounded-lg shadow-lg z-50 overflow-hidden">
                <div className="py-1">
                  <div className="px-4 py-2 text-xs font-semibold text-text-secondary bg-background border-b border-border">
                    {t('header.selectColorScheme')}
                  </div>
                  {availableSchemes.map((scheme) => {
                    // Map scheme IDs to translation keys
                    const schemeMap: Record<string, string> = {
                      'blue-ocean': 'colorSchemes.blueOcean',
                      'green-forest': 'colorSchemes.greenForest',
                      'purple-mystic': 'colorSchemes.purpleMystic',
                      'orange-sunset': 'colorSchemes.orangeSunset',
                      'pink-blossom': 'colorSchemes.pinkBlossom',
                      'gray-modern': 'colorSchemes.grayModern'
                    }
                    
                    return (
                      <button
                        key={scheme.id}
                        onClick={() => handleColorSchemeSelect(scheme.id)}
                        className={`w-full text-left px-4 py-2.5 text-sm transition-all ${
                          colorScheme.id === scheme.id
                            ? 'bg-primary text-white font-medium'
                            : 'text-text hover:bg-background'
                        }`}
                      >
                        <div className="flex items-center justify-between">
                          <span>{t(schemeMap[scheme.id] || 'colorSchemes.blueOcean')}</span>
                          <div className="flex space-x-1">
                            <div 
                              className="w-3 h-3 rounded-full border border-white/20" 
                              style={{ backgroundColor: scheme.colors.primary }}
                            />
                            <div 
                              className="w-3 h-3 rounded-full border border-white/20" 
                              style={{ backgroundColor: scheme.colors.secondary }}
                            />
                            <div 
                              className="w-3 h-3 rounded-full border border-white/20" 
                              style={{ backgroundColor: scheme.colors.accent }}
                            />
                          </div>
                        </div>
                      </button>
                    )
                  })}
                </div>
              </div>
            )}
          </div>

          {/* 暗黑/明亮模式切换 */}
          <button
            onClick={toggleMode}
            className="p-2 text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
            title={mode === 'light' ? t('header.toggleDarkMode') : t('header.toggleLightMode')}
          >
            {mode === 'light' ? (
              <Moon className="w-5 h-5" />
            ) : (
              <Sun className="w-5 h-5" />
            )}
          </button>

          {/* 登出按钮 */}
          <button
            onClick={handleLogout}
            className="px-4 py-2 text-sm text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
          >
            {t('common.logout')}
          </button>
        </div>
      </div>
    </header>
  )
}

export default Header
