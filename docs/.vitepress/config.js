import { defineConfig } from 'vitepress'

export default defineConfig({
  // Site metadata
  title: 'Synbot',
  description: 'Personal AI Assistant built with Rust',
  // Head configuration
  head: [
    ['link', { rel: 'icon', href: '/favicon.ico' }],
    ['meta', { name: 'theme-color', content: '#3eaf7c' }],
    ['meta', { name: 'og:type', content: 'website' }],
    ['meta', { name: 'og:locale', content: 'en_US' }],
    ['meta', { name: 'og:site_name', content: 'Synbot Documentation' }],
    // Script for theme switching
    ['script', { src: '/.vitepress/theme/scripts/theme.js' }]
  ],
  lang: 'en-US',
  base: '',
  locales: {
    root: {
      label: 'English',
      lang: 'en',
      themeConfig: {
        nav: [
          { text: 'Home', link: '/' },
          { text: 'Document', link: '/getting-started/installation' }
        ],
        sidebar: [
          {
            text: 'Getting Started',
            collapsed: false,
            items: [
              { text: 'Installation', link: '/getting-started/installation' },
              { text: 'Configuration', link: '/getting-started/configuration' },
              { text: 'Running Synbot', link: '/getting-started/running' },
              { text: 'First Steps', link: '/getting-started/first-steps' }
            ]
          },
          {
            text: 'User Guide',
            collapsed: false,
            items: [
              { text: 'Channels', link: '/user-guide/channels' },
              { text: 'Tools', link: '/user-guide/tools' },
              { text: 'Permissions', link: '/user-guide/permissions' }
            ]
          },
          {
            text: 'Developer Guide',
            collapsed: false,
            items: [
              { text: 'Architecture', link: '/developer-guide/architecture' }
            ]
          },
          {
            text: 'Examples',
            collapsed: false,
            items: [
              { text: 'Basic Configuration', link: '/examples/basic-config' }
            ]
          }
        ],
      }
    },
    zh: {
      label: '中文',
      lang: 'zh',
      link: '/zh/',
      themeConfig: {
        nav: [
          { text: '首页', link: '/zh/' },
          { text: '文档', link: '/zh/getting-started/installation' }
        ],
        sidebar: [
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
        ],
      }
    }
  },
  // Theme configuration
  themeConfig: {
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