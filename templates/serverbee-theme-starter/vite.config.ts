import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
export default defineConfig({
  base: '/', // REQUIRED — see spec §9.1
  plugins: [react()],
  build: { outDir: 'dist', emptyOutDir: true }
})
