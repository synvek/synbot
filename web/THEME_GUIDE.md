# 主题系统使用指南

## 概述

本系统实现了完整的暗黑主题和明亮主题支持，提供了6套配色方案，并默认激活第一套（蓝色海洋）。用户可以通过Header上的按钮快速切换主题模式和配色方案。

## 最新改进 (v2.0)

### 1. 深度定制的暗黑模式
- 每个配色方案都有独特的暗黑模式配色，而不是统一的暗色背景
- 更深邃的背景色，提供更好的对比度和视觉舒适度
- 优化了文字和边框颜色，确保在暗黑模式下的可读性

### 2. 增强的视觉反馈
- 添加了平滑的颜色过渡动画（0.2s ease-in-out）
- 改进了焦点状态指示器，符合可访问性标准
- 优化了阴影效果，在暗黑模式下更加自然

### 3. 改进的组件样式
- Sidebar激活状态使用主色背景和白色文字，更加醒目
- 下拉菜单增加了更好的视觉层次和间距
- 聊天消息气泡在暗黑模式下对比度更高

### 4. 自定义滚动条
- 滚动条颜色自动适配主题
- 在暗黑模式下提供更好的视觉体验

## 功能特性

### 1. 主题模式
- **明亮模式**：适合白天或光线充足的环境
- **暗黑模式**：适合夜间或光线较暗的环境，减少眼睛疲劳

### 2. 配色方案（6套）
1. **蓝色海洋** (默认) - 专业、科技感
2. **绿色森林** - 自然、清新
3. **紫色神秘** - 优雅、创意
4. **橙色日落** - 温暖、活力
5. **粉色花语** - 柔和、浪漫
6. **灰色现代** - 简约、商务

### 3. 自动保存
- 用户选择的主题模式和配色方案会自动保存到localStorage
- 下次访问时会自动恢复上次的设置

## 使用方法

### 1. 快速切换
在Header右上角有两个按钮：
- **配色方案按钮**：点击选择不同的配色方案
- **主题模式按钮**：点击切换明亮/暗黑模式

### 2. 详细设置
在Config页面中，可以：
- 查看所有配色方案的预览
- 详细查看当前主题的颜色值
- 批量切换主题模式和配色方案

## 技术实现

### 1. 核心组件
- `ThemeContext`：主题状态管理
- `ThemeProvider`：主题提供者，包裹整个应用
- `useTheme` hook：在组件中使用主题

### 2. CSS变量系统
系统使用CSS变量动态应用主题颜色：
```css
:root {
  --color-primary: #3b82f6;
  --color-secondary: #1e40af;
  --color-background: #f8fafc;
  --color-text: #1e293b;
  /* ... 其他颜色变量 */
}

.dark {
  --color-background: #020617;
  --color-text: #f1f5f9;
  /* ... 暗黑模式颜色覆盖 */
}
```

### 3. 平滑过渡效果
所有颜色相关的CSS属性都应用了0.2s的过渡动画：
```css
* {
  transition-property: background-color, border-color, color, fill, stroke;
  transition-timing-function: ease-in-out;
  transition-duration: 0.2s;
}
```

### 4. 可访问性增强
- 焦点状态使用2px的主色轮廓线
- 确保所有配色方案的对比度符合WCAG AA标准
- 自定义滚动条样式适配主题

### 5. Tailwind集成
在`tailwind.config.js`中配置了颜色映射：
```javascript
colors: {
  primary: 'var(--color-primary)',
  secondary: 'var(--color-secondary)',
  background: 'var(--color-background)',
  text: 'var(--color-text)',
  // ... 其他颜色
}
```

## 在组件中使用主题

### 1. 使用主题hook
```typescript
import { useTheme } from '../contexts/ThemeContext'

function MyComponent() {
  const { mode, colorScheme, toggleMode, setColorScheme } = useTheme()
  
  return (
    <div className="bg-surface text-text border-border">
      {/* 使用主题颜色类名 */}
    </div>
  )
}
```

### 2. 可用的CSS类名
- `bg-primary`, `bg-secondary`, `bg-accent`
- `bg-background`, `bg-surface`
- `text-text`, `text-text-secondary`
- `border-border`
- `text-success`, `text-warning`, `text-error`, `text-info`

### 3. 动态样式
```typescript
// 根据主题模式应用不同样式
<div className={mode === 'dark' ? 'dark:bg-gray-900' : 'bg-white'}>
```

## 扩展主题系统

### 1. 添加新的配色方案
在`ThemeContext.tsx`中的`colorSchemes`数组添加新的配色方案：
```typescript
{
  id: 'new-scheme',
  name: '新方案',
  colors: {
    primary: '#your-color',
    secondary: '#your-color',
    // ... 其他颜色
  }
}
```

### 2. 自定义组件样式
确保自定义组件使用主题颜色类名，而不是硬编码的颜色值：
```typescript
// 正确
<div className="bg-surface text-text border-border">

// 避免
<div className="bg-white text-gray-900 border-gray-300">
```

### 3. 图标和图片
对于图标，使用`lucide-react`图标库，它会自动适应文本颜色：
```typescript
import { Sun, Moon } from 'lucide-react'

<Sun className="w-5 h-5 text-text" />
```

## 最佳实践

1. **一致性**：在整个应用中使用相同的主题颜色类名
2. **可访问性**：确保颜色对比度符合WCAG AA标准（4.5:1 for normal text）
3. **性能**：主题切换使用CSS变量，性能开销小
4. **用户体验**：提供平滑的主题切换过渡效果（0.2s）
5. **焦点状态**：确保所有交互元素都有清晰的焦点指示器
6. **暗黑模式优化**：在暗黑模式下使用更深的背景色和更柔和的阴影

## 配色方案详情

### 明亮模式
- **蓝色海洋**：专业、科技感，适合企业应用
- **绿色森林**：自然、清新，适合环保、健康类应用
- **紫色神秘**：优雅、创意，适合设计、艺术类应用
- **橙色日落**：温暖、活力，适合社交、娱乐类应用
- **粉色花语**：柔和、浪漫，适合时尚、美妆类应用
- **灰色现代**：简约、商务，适合专业工具和B2B应用

### 暗黑模式
每个配色方案在暗黑模式下都有独特的背景色调：
- **蓝色海洋**：深蓝黑色背景 (#020617)
- **绿色森林**：深绿黑色背景 (#021208)
- **紫色神秘**：深紫黑色背景 (#0c0314)
- **橙色日落**：深橙黑色背景 (#120802)
- **粉色花语**：深粉黑色背景 (#0f0314)
- **灰色现代**：纯黑色背景 (#030712)

## 故障排除

### 1. 主题不生效
- 检查是否在`Layout.tsx`中包裹了`ThemeProvider`
- 检查CSS变量是否正确设置
- 查看浏览器控制台是否有错误

### 2. 颜色不一致
- 确保使用主题颜色类名，而不是硬编码颜色
- 检查Tailwind配置是否正确映射CSS变量

### 3. 主题切换卡顿
- 减少同时变化的CSS属性数量
- 使用`transition-colors`类添加平滑过渡

## 浏览器支持

- 支持所有现代浏览器（Chrome, Firefox, Safari, Edge）
- 依赖CSS变量和localStorage功能
- 不支持IE11及更早版本