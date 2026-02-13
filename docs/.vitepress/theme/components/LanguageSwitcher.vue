<template>
  <div class="language-switcher">
    <a
      href="/docs/en/"
      :class="['language-link', { active: currentLang === 'en' }]"
      @click.prevent="switchLanguage('en')"
    >
      English
    </a>
    <span class="separator">/</span>
    <a
      href="/docs/zh/"
      :class="['language-link', { active: currentLang === 'zh' }]"
      @click.prevent="switchLanguage('zh')"
    >
      中文
    </a>
  </div>
</template>

<script setup>
import { ref, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vitepress'

const route = useRoute()
const router = useRouter()
const currentLang = ref('en')

// Detect current language from path
const detectLanguage = () => {
  const path = route.path
  if (path.startsWith('/zh/')) {
    currentLang.value = 'zh'
  } else if (path.startsWith('/en/')) {
    currentLang.value = 'en'
  } else {
    currentLang.value = 'en' // default
  }
}

// Switch language
const switchLanguage = (lang) => {
  const currentPath = route.path
  let newPath
  
  if (lang === 'en') {
    // Switch to English
    if (currentPath.startsWith('/zh/')) {
      newPath = currentPath.replace('/zh/', '/en/')
    } else if (!currentPath.startsWith('/en/')) {
      newPath = `/en${currentPath === '/' ? '' : currentPath}`
    } else {
      newPath = currentPath
    }
  } else if (lang === 'zh') {
    // Switch to Chinese
    if (currentPath.startsWith('/en/')) {
      newPath = currentPath.replace('/en/', '/zh/')
    } else if (!currentPath.startsWith('/zh/')) {
      newPath = `/zh${currentPath === '/' ? '' : currentPath}`
    } else {
      newPath = currentPath
    }
  }
  
  if (newPath !== currentPath) {
    router.go(newPath)
  }
}

// Update language when route changes
onMounted(detectLanguage)
watch(() => route.path, detectLanguage)
</script>

<style scoped>
.language-switcher {
  display: flex;
  align-items: center;
  gap: 4px;
}

.language-link {
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
  font-weight: 500;
  text-decoration: none;
  color: var(--vp-c-language-inactive);
  transition: all 0.2s ease;
  cursor: pointer;
}

.language-link:hover {
  color: var(--vp-c-text-1);
  background-color: var(--vp-c-bg-soft);
}

.language-link.active {
  color: var(--vp-c-language-active);
  background-color: var(--vp-c-bg-soft);
}

.separator {
  color: var(--vp-c-divider);
  user-select: none;
}

@media (max-width: 768px) {
  .language-switcher {
    gap: 2px;
  }
  
  .language-link {
    padding: 4px 6px;
    font-size: 13px;
  }
}
</style>