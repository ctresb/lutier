import { defineConfig } from 'vite'

// config pensada pro tauri: porta fixa, sem limpar tela do cargo
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2022',
    minify: 'esbuild',
    sourcemap: false,
    chunkSizeWarningLimit: 600,
  },
})
