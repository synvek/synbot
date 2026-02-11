import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'

// 定义6套配色方案
export type ColorScheme = {
  id: string
  name: string
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

export const colorSchemes: ColorScheme[] = [
  {
    id: 'blue-ocean',
    name: '蓝色海洋',
    colors: {
      primary: '#3b82f6',
      secondary: '#1e40af',
      accent: '#60a5fa',
      background: '#f8fafc',
      surface: '#ffffff',
      text: '#1e293b',
      textSecondary: '#64748b',
      border: '#e2e8f0',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  },
  {
    id: 'green-forest',
    name: '绿色森林',
    colors: {
      primary: '#10b981',
      secondary: '#047857',
      accent: '#34d399',
      background: '#f0fdf4',
      surface: '#ffffff',
      text: '#1e293b',
      textSecondary: '#64748b',
      border: '#d1fae5',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  },
  {
    id: 'purple-mystic',
    name: '紫色神秘',
    colors: {
      primary: '#8b5cf6',
      secondary: '#7c3aed',
      accent: '#a78bfa',
      background: '#faf5ff',
      surface: '#ffffff',
      text: '#1e293b',
      textSecondary: '#64748b',
      border: '#e9d5ff',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  },
  {
    id: 'orange-sunset',
    name: '橙色日落',
    colors: {
      primary: '#f97316',
      secondary: '#ea580c',
      accent: '#fb923c',
      background: '#fff7ed',
      surface: '#ffffff',
      text: '#1e293b',
      textSecondary: '#64748b',
      border: '#fed7aa',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  },
  {
    id: 'pink-blossom',
    name: '粉色花语',
    colors: {
      primary: '#ec4899',
      secondary: '#db2777',
      accent: '#f472b6',
      background: '#fdf2f8',
      surface: '#ffffff',
      text: '#1e293b',
      textSecondary: '#64748b',
      border: '#fbcfe8',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  },
  {
    id: 'gray-modern',
    name: '灰色现代',
    colors: {
      primary: '#6b7280',
      secondary: '#4b5563',
      accent: '#9ca3af',
      background: '#f9fafb',
      surface: '#ffffff',
      text: '#1f2937',
      textSecondary: '#6b7280',
      border: '#e5e7eb',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
      info: '#3b82f6'
    }
  }
]

// 暗黑模式配色方案 - 为每个方案定制更深邃且有特色的暗黑配色
export const darkColorSchemes: ColorScheme[] = [
  {
    id: 'blue-ocean',
    name: '蓝色海洋',
    colors: {
      primary: '#60a5fa',
      secondary: '#3b82f6',
      accent: '#93c5fd',
      background: '#020617',
      surface: '#0f172a',
      text: '#f1f5f9',
      textSecondary: '#94a3b8',
      border: '#1e293b',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  },
  {
    id: 'green-forest',
    name: '绿色森林',
    colors: {
      primary: '#34d399',
      secondary: '#10b981',
      accent: '#6ee7b7',
      background: '#021208',
      surface: '#052e16',
      text: '#f0fdf4',
      textSecondary: '#86efac',
      border: '#14532d',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  },
  {
    id: 'purple-mystic',
    name: '紫色神秘',
    colors: {
      primary: '#a78bfa',
      secondary: '#8b5cf6',
      accent: '#c4b5fd',
      background: '#0c0314',
      surface: '#1e1b4b',
      text: '#faf5ff',
      textSecondary: '#c4b5fd',
      border: '#312e81',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  },
  {
    id: 'orange-sunset',
    name: '橙色日落',
    colors: {
      primary: '#fb923c',
      secondary: '#f97316',
      accent: '#fdba74',
      background: '#120802',
      surface: '#431407',
      text: '#fff7ed',
      textSecondary: '#fed7aa',
      border: '#7c2d12',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  },
  {
    id: 'pink-blossom',
    name: '粉色花语',
    colors: {
      primary: '#f472b6',
      secondary: '#ec4899',
      accent: '#f9a8d4',
      background: '#0f0314',
      surface: '#500724',
      text: '#fdf2f8',
      textSecondary: '#fbcfe8',
      border: '#831843',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  },
  {
    id: 'gray-modern',
    name: '灰色现代',
    colors: {
      primary: '#9ca3af',
      secondary: '#6b7280',
      accent: '#d1d5db',
      background: '#030712',
      surface: '#111827',
      text: '#f9fafb',
      textSecondary: '#9ca3af',
      border: '#374151',
      success: '#22c55e',
      warning: '#fbbf24',
      error: '#f87171',
      info: '#60a5fa'
    }
  }
]

type ThemeMode = 'light' | 'dark'

interface ThemeContextType {
  mode: ThemeMode
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
  const [mode, setMode] = useState<ThemeMode>(() => {
    const saved = localStorage.getItem('theme-mode')
    return (saved as ThemeMode) || 'light'
  })

  const [colorSchemeId, setColorSchemeId] = useState<string>(() => {
    const saved = localStorage.getItem('theme-scheme')
    return saved || 'blue-ocean'
  })

  const availableSchemes = mode === 'light' ? colorSchemes : darkColorSchemes
  const colorScheme = availableSchemes.find(s => s.id === colorSchemeId) || availableSchemes[0]

  const toggleMode = () => {
    setMode(prev => {
      const newMode = prev === 'light' ? 'dark' : 'light'
      localStorage.setItem('theme-mode', newMode)
      return newMode
    })
  }

  const handleSetMode = (newMode: ThemeMode) => {
    setMode(newMode)
    localStorage.setItem('theme-mode', newMode)
  }

  const handleSetColorScheme = (schemeId: string) => {
    setColorSchemeId(schemeId)
    localStorage.setItem('theme-scheme', schemeId)
  }

  // 应用主题到CSS变量
  useEffect(() => {
    const root = document.documentElement
    
    // 设置CSS变量
    Object.entries(colorScheme.colors).forEach(([key, value]) => {
      root.style.setProperty(`--color-${key}`, value)
    })

    // 设置主题类
    if (mode === 'dark') {
      root.classList.add('dark')
    } else {
      root.classList.remove('dark')
    }
  }, [mode, colorScheme])

  return (
    <ThemeContext.Provider
      value={{
        mode,
        colorScheme,
        setMode: handleSetMode,
        setColorScheme: handleSetColorScheme,
        toggleMode,
        availableSchemes
      }}
    >
      {children}
    </ThemeContext.Provider>
  )
}