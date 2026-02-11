import React, { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { apiClient } from '../api/client'
import { useTheme, colorSchemes, darkColorSchemes } from '../contexts/ThemeContext'
import { Sun, Moon, Palette, ChevronDown } from 'lucide-react'

const Header: React.FC = () => {
  const navigate = useNavigate()
  const { mode, colorScheme, toggleMode, setColorScheme, availableSchemes } = useTheme()
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
    return scheme?.name || '蓝色海洋'
  }

  return (
    <header className="bg-surface shadow-sm border-b border-border">
      <div className="px-6 py-4 flex justify-between items-center">
        <h1 className="text-2xl font-bold text-text">
          Web Admin Dashboard
        </h1>
        
        <div className="flex items-center space-x-4">
          {/* 主题切换按钮 */}
          <div className="relative">
            <button
              onClick={() => setShowColorMenu(!showColorMenu)}
              className="flex items-center space-x-2 px-3 py-2 text-sm text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
            >
              <Palette className="w-4 h-4" />
              <span>{getCurrentSchemeName()}</span>
              <ChevronDown className="w-4 h-4" />
            </button>
            
            {showColorMenu && (
              <div className="absolute right-0 mt-2 w-56 bg-surface border border-border rounded-lg shadow-lg z-50 overflow-hidden">
                <div className="py-1">
                  <div className="px-4 py-2 text-xs font-semibold text-text-secondary bg-background border-b border-border">
                    选择配色方案
                  </div>
                  {availableSchemes.map((scheme) => (
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
                        <span>{scheme.name}</span>
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
                  ))}
                </div>
              </div>
            )}
          </div>

          {/* 暗黑/明亮模式切换 */}
          <button
            onClick={toggleMode}
            className="p-2 text-text hover:text-primary hover:bg-surface rounded-md transition-colors border border-border"
            title={mode === 'light' ? '切换到暗黑模式' : '切换到明亮模式'}
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
            Logout
          </button>
        </div>
      </div>
    </header>
  )
}

export default Header
