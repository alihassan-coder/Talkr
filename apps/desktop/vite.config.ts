import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@talkr/ui': path.resolve(__dirname, '../../packages/ui/src'),
      '@talkr/model-catalog': path.resolve(__dirname, '../../packages/model-catalog'),
      '@talkr/config': path.resolve(__dirname, '../../packages/config')
    }
  },
  build: {
    outDir: 'dist',
    sourcemap: true
  },
  server: {
    port: 1420,
    strictPort: true
  },
  envPrefix: ['VITE_', 'TAURI_']
})