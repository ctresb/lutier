import { defineConfig } from 'vite'

// config pensada pro tauri: porta fixa, sem limpar tela do cargo
export default defineConfig({
  clearScreen: false,
  server: { port: 1430, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_'],
  assetsInclude: ['**/*.glb'],
  build: {
    target: 'es2022',
    minify: 'esbuild',
    sourcemap: false,
    chunkSizeWarningLimit: 600,
  },
})
