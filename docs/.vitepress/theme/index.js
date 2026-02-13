import DefaultTheme from 'vitepress/theme'
import './styles/custom.css'
import LanguageSwitcher from './components/LanguageSwitcher.vue'
import ThemeToggle from './components/ThemeToggle.vue'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    // Register global components
    app.component('LanguageSwitcher', LanguageSwitcher)
    app.component('ThemeToggle', ThemeToggle)
  }
}