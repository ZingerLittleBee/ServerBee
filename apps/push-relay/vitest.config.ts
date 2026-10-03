import { generateKeyPairSync } from 'node:crypto'
import { cloudflareTest } from '@cloudflare/vitest-pool-workers'
import type { Request as MiniflareRequest } from 'miniflare'
import { defineConfig } from 'vitest/config'

const key = generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.jsonc' },
      miniflare: {
        outboundService(request: MiniflareRequest) {
          const url = new URL(request.url)
          if (url.hostname !== 'api.sandbox.push.apple.com' || request.method !== 'POST') {
            throw new Error('Unexpected outbound request')
          }
          return new Response(null, { status: 200 })
        },
        bindings: {
          APNS_TEAM_ID: 'TESTTEAM01',
          APNS_KEY_ID: 'TESTKEY001',
          APNS_PRIVATE_KEY: key.privateKey.export({ format: 'pem', type: 'pkcs8' }).toString(),
          APNS_TOPIC: 'com.serverbee.mobile'
        }
      }
    })
  ],
  test: { include: ['tests/**/*.test.ts'], testTimeout: 10_000 }
})
