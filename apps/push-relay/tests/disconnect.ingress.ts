import { generateKeyPairSync } from 'node:crypto'
import { type ClientRequest, request as httpRequest } from 'node:http'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { Miniflare, type Request as MiniflareRequest } from 'miniflare'
import { expect, test } from 'vitest'
import { unstable_readConfig } from 'wrangler'
import vector from '../../../tests/fixtures/push-envelope-v1.json'

function send(url: string): { client: ClientRequest; done: Promise<number> } {
  let resolve: (status: number) => void = () => undefined
  const done = new Promise<number>((finish) => {
    resolve = finish
  })
  const client = httpRequest(
    url,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      agent: false
    },
    (response) => {
      response.resume()
      response.on('end', () => resolve(response.statusCode ?? 0))
    }
  )
  client.on('error', () => resolve(0))
  client.end(
    JSON.stringify({
      device_token: 'a'.repeat(64),
      environment: 'sandbox',
      event_id: '11111111-1111-4111-8111-111111111111',
      expires_at: Math.floor(Date.now() / 1000) + 1800,
      envelope: vector.envelope
    })
  )
  return { client, done }
}

async function until(predicate: () => boolean, timeout: number): Promise<void> {
  const end = performance.now() + timeout
  while (!predicate()) {
    if (performance.now() >= end) {
      throw new Error('Timed out awaiting real runtime ingress')
    }
    await sleep(10)
  }
}

test('real HTTP disconnect frees an occupied Relay slot before the APNs deadline', async () => {
  const config = unstable_readConfig({ config: fileURLToPath(new URL('../wrangler.jsonc', import.meta.url)) })
  expect(config.compatibility_flags).toContain('enable_request_signal')
  const key = generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
  const bundle = await build({
    entryPoints: [fileURLToPath(new URL('../src/relay.ts', import.meta.url))],
    tsconfig: fileURLToPath(new URL('../tsconfig.json', import.meta.url)),
    bundle: true,
    write: false,
    platform: 'browser',
    format: 'esm',
    target: 'es2022'
  })
  let entered = 0
  const release: (() => void)[] = []
  const clients: ClientRequest[] = []
  const runtime = new Miniflare({
    modules: true,
    script: bundle.outputFiles[0].text,
    compatibilityDate: config.compatibility_date,
    compatibilityFlags: config.compatibility_flags,
    host: '127.0.0.1',
    port: 0,
    // Use native workerd ingress instead of Miniflare's extra routing Worker.
    unsafeDirectSockets: [{ host: '127.0.0.1', port: 0 }],
    bindings: {
      APNS_TEAM_ID: 'TESTTEAM01',
      APNS_KEY_ID: 'TESTKEY001',
      APNS_TOPIC: 'app.serverbee',
      APNS_PRIVATE_KEY: key.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString()
    },
    async outboundService(request: MiniflareRequest) {
      expect(new URL(request.url).hostname).toBe('api.sandbox.push.apple.com')
      await request.text()
      entered += 1
      if (entered <= 32) {
        await new Promise<void>((resolve) => {
          release.push(resolve)
        })
      }
      return new Response(null)
    }
  })
  try {
    const url = `${(await runtime.unsafeGetDirectURL()).origin}/v1/send`
    const held = Array.from({ length: 32 }, () => send(url))
    clients.push(...held.map((value) => value.client))
    await until(() => entered === 32, 5000)
    const busy = send(url)
    clients.push(busy.client)
    expect(await busy.done).toBe(503)
    const start = performance.now()
    const socket = held[0].client.socket
    if (!socket) {
      throw new Error('Missing live HTTP socket')
    }
    // An HTTP/1 half-close can still await a response. TCP reset is unambiguous cancellation.
    socket.resetAndDestroy()
    let status = 503
    while (status === 503 && performance.now() - start < 2500) {
      await sleep(50)
      const probe = send(url)
      clients.push(probe.client)
      status = await probe.done
    }
    expect(status).toBe(200)
    expect(entered).toBe(33)
    expect(performance.now() - start).toBeLessThan(2500)
  } finally {
    for (const client of clients) {
      client.destroy()
    }
    for (const finish of release) {
      finish()
    }
    await runtime.dispose()
  }
})
