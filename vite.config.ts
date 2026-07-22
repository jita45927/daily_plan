import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src')
    }
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        contextMenu: resolve(__dirname, 'context-menu.html'),
        trashContextMenu: resolve(__dirname, 'trash-context-menu.html'),
        snapLine: resolve(__dirname, 'snap-line.html'),
        desktopAnalyze: resolve(__dirname, 'desktop-analyze.html'),
        downloadsAnalyze: resolve(__dirname, 'downloads-analyze.html'),
        welcome: resolve(__dirname, 'welcome.html'),
      }
    }
  }
})
