import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'

const host = process.env.TAURI_DEV_HOST
const rootDir = fileURLToPath(new URL('.', import.meta.url))

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  build: {
    rollupOptions: {
      input: {
        main: resolve(rootDir, 'index.html'),
        whiteboard: resolve(rootDir, 'whiteboard.html'),
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // The dev server only needs the frontend sources. Watching the vault,
      // screenshots, docs, build output or Rust target during a session lets
      // chokidar/HMR accumulate state (a 9 GB Vite process was observed after
      // rapid edits + builds). Keep those directories out of the watcher.
      ignored: [
        '**/src-tauri/**',
        '**/sample-vault/**',
        '**/shots/**',
        '**/docs/**',
        '**/dist/**',
        '**/.yebian/**',
      ],
    },
  },
})
