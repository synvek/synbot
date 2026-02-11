import React from 'react'
import { useTheme, colorSchemes, darkColorSchemes } from '../contexts/ThemeContext'

const ThemePreview: React.FC = () => {
  const { mode, colorScheme, setColorScheme, setMode } = useTheme()

  return (
    <div className="bg-surface border border-border rounded-lg p-6">
      <h2 className="text-xl font-bold text-text mb-4">主题预览</h2>
      
      <div className="mb-6">
        <h3 className="text-lg font-semibold text-text mb-3">模式选择</h3>
        <div className="flex space-x-4">
          <button
            onClick={() => setMode('light')}
            className={`px-4 py-2 rounded-lg transition-colors ${
              mode === 'light'
                ? 'bg-primary text-white'
                : 'bg-surface border border-border text-text hover:bg-surface/80'
            }`}
          >
            明亮模式
          </button>
          <button
            onClick={() => setMode('dark')}
            className={`px-4 py-2 rounded-lg transition-colors ${
              mode === 'dark'
                ? 'bg-primary text-white'
                : 'bg-surface border border-border text-text hover:bg-surface/80'
            }`}
          >
            暗黑模式
          </button>
        </div>
      </div>

      <div className="mb-6">
        <h3 className="text-lg font-semibold text-text mb-3">配色方案</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {(mode === 'light' ? colorSchemes : darkColorSchemes).map((scheme) => (
            <div
              key={scheme.id}
              className={`border rounded-lg p-4 cursor-pointer transition-all hover:scale-[1.02] ${
                colorScheme.id === scheme.id
                  ? 'border-primary ring-2 ring-primary/20'
                  : 'border-border hover:border-primary/50'
              }`}
              onClick={() => setColorScheme(scheme.id)}
            >
              <div className="flex justify-between items-center mb-3">
                <h4 className="font-medium text-text">{scheme.name}</h4>
                {colorScheme.id === scheme.id && (
                  <span className="text-xs bg-primary text-white px-2 py-1 rounded">
                    当前使用
                  </span>
                )}
              </div>
              
              <div className="space-y-2">
                <div className="flex space-x-1">
                  <div 
                    className="flex-1 h-8 rounded" 
                    style={{ backgroundColor: scheme.colors.primary }}
                    title="主色"
                  />
                  <div 
                    className="flex-1 h-8 rounded" 
                    style={{ backgroundColor: scheme.colors.secondary }}
                    title="辅色"
                  />
                  <div 
                    className="flex-1 h-8 rounded" 
                    style={{ backgroundColor: scheme.colors.accent }}
                    title="强调色"
                  />
                </div>
                
                <div className="grid grid-cols-2 gap-2">
                  <div className="text-xs">
                    <div className="font-medium text-text-secondary">背景</div>
                    <div 
                      className="h-4 rounded mt-1 border border-border" 
                      style={{ backgroundColor: scheme.colors.background }}
                    />
                  </div>
                  <div className="text-xs">
                    <div className="font-medium text-text-secondary">表面</div>
                    <div 
                      className="h-4 rounded mt-1 border border-border" 
                      style={{ backgroundColor: scheme.colors.surface }}
                    />
                  </div>
                  <div className="text-xs">
                    <div className="font-medium text-text-secondary">文字</div>
                    <div 
                      className="h-4 rounded mt-1 border border-border" 
                      style={{ backgroundColor: scheme.colors.text }}
                    />
                  </div>
                  <div className="text-xs">
                    <div className="font-medium text-text-secondary">边框</div>
                    <div 
                      className="h-4 rounded mt-1 border border-border" 
                      style={{ backgroundColor: scheme.colors.border }}
                    />
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="bg-surface border border-border rounded-lg p-4">
        <h3 className="text-lg font-semibold text-text mb-3">当前主题详情</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {Object.entries(colorScheme.colors).map(([key, value]) => (
            <div key={key} className="text-sm">
              <div className="font-medium text-text-secondary capitalize">
                {key.replace(/([A-Z])/g, ' $1').trim()}
              </div>
              <div className="flex items-center gap-2 mt-1">
                <div 
                  className="w-6 h-6 rounded border border-border" 
                  style={{ backgroundColor: value }}
                />
                <code className="text-xs text-text">{value}</code>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default ThemePreview