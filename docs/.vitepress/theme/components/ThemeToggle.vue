<template>
  <button
    class="theme-toggle"
    :aria-label="`Switch to ${isDark ? 'light' : 'dark'} mode`"
    @click="toggleTheme"
  >
    <svg
      v-if="isDark"
      class="sun-icon"
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="12" cy="12" r="5"></circle>
      <line x1="12" y1="1" x2="12" y2="3"></line>
      <line x1="12" y1="21" x2="12" y2="23"></line>
      <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line>
      <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line>
      <line x1="1" y1="12" x2="3" y2="12"></line>
      <line x1="21" y1="12" x2="23" y2="12"></line>
      <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line>
      <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line>
    </svg>
    <svg
      v-else
      class="moon-icon"
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
    </svg>
  </button>
</template>

<script setup>
import { ref, onMounted } from 'vue'

const isDark = ref(false)

// Check current theme
const checkTheme = () => {
  const saved = localStorage.getItem('vitepress-theme-appearance')
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  
  if (saved === 'dark' || (!saved && prefersDark)) {
    isDark.value = true
  } else {
    isDark.value = false
  }
}

// Toggle theme
const toggleTheme = () => {
  const newTheme = isDark.value ? 'light' : 'dark'
  
  // Update localStorage
  localStorage.setItem('vitepress-theme-appearance', newTheme)
  
  // Update DOM
  if (newTheme === 'dark') {
    document.documentElement.classList.add('dark')
  } else {
    document.documentElement.classList.remove('dark')
  }
  
  // Update state
  isDark.value = !isDark.value
  
  // Dispatch event for other components
  window.dispatchEvent(new CustomEvent('theme-change', { detail: newTheme }))
}

// Listen for system theme changes
const watchSystemTheme = () => {
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
  
  const handleChange = (e) => {
    // Only change if user hasn't set a preference
    if (!localStorage.getItem('vitepress-theme-appearance')) {
      if (e.matches) {
        document.documentElement.classList.add('dark')
        isDark.value = true
      } else {
        document.documentElement.classList.remove('dark')
        isDark.value = false
      }
    }
  }
  
  mediaQuery.addEventListener('change', handleChange)
  
  // Cleanup
  onUnmounted(() => {
    mediaQuery.removeEventListener('change', handleChange)
  })
}

// Initialize
onMounted(() => {
  checkTheme()
  watchSystemTheme()
})
</script>

<style scoped>
.theme-toggle {
  background: none;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  padding: 8px;
  cursor: pointer;
  color: var(--vp-c-text-2);
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
}

.theme-toggle:hover {
  color: var(--vp-c-text-1);
  background-color: var(--vp-c-bg-soft);
  border-color: var(--vp-c-divider-light);
}

.sun-icon,
.moon-icon {
  width: 16px;
  height: 16px;
}

@media (max-width: 768px) {
  .theme-toggle {
    width: 36px;
    height: 36px;
    padding: 6px;
  }
}
</style>