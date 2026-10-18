import { defineConfig } from 'vite'

// Порт фиксирован: его ждёт tauri.conf.json (devUrl). Выше 15000 — вне динамического диапазона Windows,
// иначе Hyper-V/WSL может зарезервировать его, и vite упадёт с EACCES.
export default defineConfig({
  clearScreen: false,
  server: { port: 17420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
})
