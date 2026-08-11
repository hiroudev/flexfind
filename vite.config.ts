import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri expects a fixed dev server. These settings keep the plain web
// dev server (`npm run dev`) working while also satisfying `tauri dev`.
// See https://v2.tauri.app/start/frontend/vite/
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [react()],

  // Prevent Vite from obscuring Rust errors during `tauri dev`.
  clearScreen: false,

  server: {
    port: 5175,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: 'ws', host, port: 5176 }
      : undefined,
    watch: {
      // Don't watch the Rust side — cargo handles that.
      ignored: ['**/src-tauri/**'],
    },
  },

  // Expose TAURI_ env vars to the frontend.
  envPrefix: ['VITE_', 'TAURI_'],

  build: {
    // Tauri uses a modern WebView2; target accordingly when building for it.
    target: process.env.TAURI_ENV_PLATFORM ? 'chrome105' : 'esnext',
    minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      input: {
        main: 'index.html',
        settings: 'settings.html',
      },
    },
  },
})
