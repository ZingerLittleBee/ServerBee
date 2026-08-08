import type { UserConfig } from 'vite'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import config from './vite.config'

const callConfig = config as unknown as (env: { command: 'serve'; mode: string }) => Promise<UserConfig> | UserConfig

function pluginNames(resolved: UserConfig): string[] {
  const flatten = (items: unknown[]): unknown[] =>
    items.flatMap((item) => (Array.isArray(item) ? flatten(item) : [item]))
  return flatten([...(resolved.plugins ?? [])])
    .map((plugin) => (plugin as { name?: string } | null)?.name)
    .filter((name): name is string => typeof name === 'string')
}

const ENV_KEYS = ['ALLOW_WRITES', 'BONEYARD_SKIP_PLUGIN', 'SERVERBEE_PROD_READONLY_API_KEY', 'SERVERBEE_PROD_URL']

describe('vite config boneyard integration', () => {
  const saved: Record<string, string | undefined> = {}

  beforeEach(() => {
    for (const key of ENV_KEYS) {
      saved[key] = process.env[key]
      delete process.env[key]
    }
  })

  afterEach(() => {
    for (const key of ENV_KEYS) {
      if (saved[key] === undefined) {
        delete process.env[key]
      } else {
        process.env[key] = saved[key]
      }
    }
  })

  it('registers the boneyard plugin in normal dev mode with the local proxy intact', async () => {
    const resolved = await callConfig({ command: 'serve', mode: 'development' })

    expect(pluginNames(resolved)).toContain('boneyard')
    expect(resolved.server?.proxy?.['/api']).toMatchObject({
      changeOrigin: true,
      target: 'http://localhost:9527',
      ws: true
    })
  })

  it('registers the boneyard plugin in prod-proxy mode without weakening the proxy guards', async () => {
    // loadEnv gives existing process.env values priority over any .env file,
    // so these stubs win without touching real credentials.
    process.env.SERVERBEE_PROD_URL = 'https://prod.example.com'
    process.env.SERVERBEE_PROD_READONLY_API_KEY = 'serverbee_test_readonly_key'

    const resolved = await callConfig({ command: 'serve', mode: 'prod-proxy' })

    expect(pluginNames(resolved)).toContain('boneyard')
    expect(resolved.define?.['import.meta.env.VITE_DEV_PROXY_TARGET']).toBe('"https://prod.example.com"')
    expect(resolved.define?.['import.meta.env.VITE_DEV_PROXY_ALLOW_WRITES']).toBe('"0"')
    // The prod-proxy entry is the guarded createDevProxy config (method
    // blocking, header stripping, WS allow-list), pointed at the stub target.
    expect(resolved.server?.proxy?.['/api']).toMatchObject({
      changeOrigin: true,
      target: 'https://prod.example.com',
      ws: true
    })
  })

  it('omits the boneyard plugin when the generation script drives the CLI itself', async () => {
    process.env.BONEYARD_SKIP_PLUGIN = '1'

    const resolved = await callConfig({ command: 'serve', mode: 'development' })

    expect(pluginNames(resolved)).not.toContain('boneyard')
    expect(resolved.server?.proxy?.['/api']).toMatchObject({ target: 'http://localhost:9527' })
  })
})
