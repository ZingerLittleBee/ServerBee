import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: { include: ['tests/**/*.ingress.ts'], environment: 'node', testTimeout: 20_000 }
})
