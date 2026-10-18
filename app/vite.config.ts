import { defineConfig } from 'vite'

// Порт фиксирован: его ждёт tauri.conf.json (devUrl).
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
})
