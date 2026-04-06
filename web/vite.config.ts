import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev: open the URL Vite prints (below). Only /api and /ws are proxied to synbot.
// Do not use synbot's web.port (e.g. 18888) for hot-reload UI — that is the Rust server, not Vite.

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    {
      name: 'synbot-print-dev-hint',
      configureServer(server) {
        server.httpServer?.once('listening', () => {
          const addr = server.httpServer?.address()
          const port =
            addr && typeof addr === 'object' ? addr.port : '(see Vite output)'
          // eslint-disable-next-line no-console
          console.log(
            `\n  [synbot web] UI dev server: http://127.0.0.1:${port}/\n` +
              `  [synbot web] /api and /ws proxy to http://127.0.0.1:18888 — run \`synbot start\` (or your configured web.port) so the API exists.\n`,
          )
        })
      },
    },
  ],
  appType: 'spa',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
  server: {
    port: 3000,
    strictPort: false,
    host: '127.0.0.1',
    open: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:18888',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://127.0.0.1:18888',
        ws: true,
      },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})
