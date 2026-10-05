/** Cross-language Server → actual Worker → APNs-boundary fixture.
 * Node/Bun only launches Miniflare; all Relay code runs in workerd.
 * No Apple keys, accounts, certificates or provider calls are used. */
import { generateKeyPairSync } from 'node:crypto'
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { Miniflare, type Request as MiniflareRequest } from 'miniflare'

const directory = process.argv[2]
if (!directory) {
  throw new Error('Missing isolated fixture directory')
}
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
const runtime = new Miniflare({
  modules: true,
  script: bundle.outputFiles[0].text,
  compatibilityDate: '2026-07-30',
  compatibilityFlags: ['enable_request_signal'],
  host: '127.0.0.1',
  port: 0,
  bindings: {
    APNS_TEAM_ID: 'TESTTEAM01',
    APNS_KEY_ID: 'TESTKEY001',
    APNS_TOPIC: 'app.serverbee',
    APNS_PRIVATE_KEY: key.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString()
  },
  async outboundService(request: MiniflareRequest) {
    const url = new URL(request.url)
    if (!['api.push.apple.com', 'api.sandbox.push.apple.com'].includes(url.hostname) || request.method !== 'POST') {
      throw new Error('Unexpected fixture outbound request')
    }
    const captured = {
      token: url.pathname.slice('/3/device/'.length),
      environment: url.hostname === 'api.sandbox.push.apple.com' ? 'sandbox' : 'production',
      headers: Object.fromEntries(request.headers),
      payload: await request.text()
    }
    let control: { status: number; reason?: string; delay_ms?: number; wait_for_release?: boolean } = { status: 200 }
    if (existsSync(`${directory}/provider.json`)) {
      control = JSON.parse(readFileSync(`${directory}/provider.json`, 'utf8'))
    }
    writeFileSync(`${directory}/provider-started.json`, JSON.stringify({ token: captured.token }))
    if (control.wait_for_release) {
      const deadline = Date.now() + 10_000
      while (!existsSync(`${directory}/release`) && Date.now() < deadline) {
        await sleep(20)
      }
    }
    if (control.delay_ms) {
      await sleep(control.delay_ms)
    }
    writeFileSync(`${directory}/provider-request.json`, JSON.stringify(captured))
    appendFileSync(`${directory}/provider-requests.jsonl`, `${JSON.stringify(captured)}\n`)
    return new Response(control.status === 200 ? null : JSON.stringify({ reason: control.reason }), {
      status: control.status
    })
  }
})
const url = (await runtime.ready).origin
writeFileSync(`${directory}/ready.json`, JSON.stringify({ url, device_token: 'a'.repeat(64), environment: 'sandbox' }))
let stopping = false
async function shutdown() {
  if (stopping) {
    return
  }
  stopping = true
  await runtime.dispose()
  process.exit(0)
}
process.on('SIGTERM', shutdown)
process.on('SIGINT', shutdown)
