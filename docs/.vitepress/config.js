import { defineConfig } from 'vitepress'

export default defineConfig({
  // Site metadata
  title: 'Synbot Documentation',
  description: 'Personal AI Assistant built with Rust',
  lang: 'en-US',
  base: '/docs/',

  // Theme configuration
  themeConfig: {
    // Site navigation
    nav: [
      { text: 'Home', link: '/' },
      { text: 'English', link: '/en/' },
      { text: '中文', link: '/zh/' },
      { text: 'GitHub', link: 'https://github.com/synbot/synbot' }
    ],

    // Sidebar configuration
    sidebar: {
      '/en/': [
        {
          text: 'Getting Started',
          collapsed: false,
          items: [
            { text: 'Installation', link: '/en/getting-started/installation' },
            { text: 'Configuration', link: '/en/getting-started/configuration' },
            { text: 'Running Synbot', link: '/en/getting-started/running' },
            { text: 'First Steps', link: '/en/getting-started/first-steps' }
          ]
        },
        {
          text: 'User Guide',
          collapsed: false,
          items: [
            { text: 'Channels', link: '/en/user-guide/channels' },
            { text: 'Tools', link: '/en/user-guide/tools' },
            { text: 'Permissions', link: '/en/user-guide/permissions' }
          ]
        },
        {
          text: 'Developer Guide',
          collapsed: false,
          items: [
            { text: 'Architecture', link: '/en/developer-guide/architecture' }
          ]
        },
        {
          text: 'Examples',
          collapsed: false,
          items: [
            { text: 'Basic Configuration', link: '/en/examples/basic-config' }
          ]
        }
      ],
      '/zh/': [
        {
          text: '入门指南',
          collapsed: false,
          items: [
            { text: '安装指南', link: '/zh/getting-started/installation' },
            { text: '配置指南', link: '/zh/getting-started/configuration' },
            { text: '运行 Synbot', link: '/zh/getting-started/running' },
            { text: '第一步', link: '/zh/getting-started/first-steps' }
          ]
        },
        {
          text: '用户指南',
          collapsed: false,
          items: [
            { text: '渠道', link: '/zh/user-guide/channels' },
            { text: '工具', link: '/zh/user-guide/tools' },
            { text: '权限', link: '/zh/user-guide/permissions' }
          ]
        },
        {
          text: '开发指南',
          collapsed: false,
          items: [
            { text: '架构', link: '/zh/developer-guide/architecture' }
          ]
        },
        {
          text: '示例',
          collapsed: false,
          items: [
            { text: '基本配置', link: '/zh/examples/basic-config' }
          ]
        }
      ]
    },

    // Social links
    socialLinks: [
      { icon: 'github', link: 'https://github.com/synbot/synbot' }
    ],

    // Footer configuration
    footer: {
      message: 'Released under the MIT License.',
      copyright: `Copyright © ${new Date().getFullYear()} Synbot Project`
    },

    // Search configuration
    search: {
      provider: 'local'
    },

    // Edit link
    editLink: {
      pattern: 'https://github.com/synbot/synbot/edit/main/docs/:path',
      text: 'Edit this page on GitHub'
    },

    // Last updated
    lastUpdated: {
      text: 'Last updated',
      formatOptions: {
        dateStyle: 'short',
        timeStyle: 'short'
      }
    },

    // Doc footer
    docFooter: {
      prev: 'Previous page',
      next: 'Next page'
    },

    // Outline
    outline: {
      level: [2, 3],
      label: 'On this page'
    }
  },

  // Markdown configuration
  markdown: {
    theme: {
      light: 'github-light',
      dark: 'github-dark'
    },
    lineNumbers: true,
    config: (md) => {
      // Add markdown-it plugins here if needed
    }
  },

  // Vite configuration
  vite: {
    server: {
      port: 5173,
      host: true
    },
    build: {
      minify: 'terser',
      chunkSizeWarningLimit: 1000
    },
    css: {
      preprocessorOptions: {
        scss: {
          additionalData: `@import "./.vitepress/theme/styles/variables.scss";`
        }
      }
    }
  },

  // Head configuration
  head: [
    ['link', { rel: 'icon', href: '/docs/favicon.ico', type: 'image/x-icon' }],
    ['meta', { name: 'theme-color', content: '#3eaf7c' }],
    ['meta', { name: 'og:type', content: 'website' }],
    ['meta', { name: 'og:locale', content: 'en_US' }],
    ['meta', { name: 'og:site_name', content: 'Synbot Documentation' }],
    // Script for theme switching
    ['script', { src: '/docs/.vitepress/theme/scripts/theme.js' }]
  ],

  // Appearance
  appearance: 'dark',

  // Clean URLs
  cleanUrls: true,

  // Ignore dead links during development
  ignoreDeadLinks: true,

  // Sitemap
  sitemap: {
    hostname: 'https://synbot.github.io'
  }
})