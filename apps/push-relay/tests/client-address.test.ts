import { afterEach, expect, test } from 'bun:test'
import { execFile } from 'node:child_process'
import { generateKeyPairSync } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'
import { serve } from 'bun'
import { type ApnsRequest, ApnsTransport } from '../src/apns'
import { createRelayFetch, normalizeIP, parseTrustedProxyIPs } from '../src/client-address'
import { Relay } from '../src/relay'
import { AppleFixture } from './fixtures'

const cleanups: (() => void | Promise<void>)[] = []
afterEach(async () => {
  for (const cleanup of cleanups.splice(0).reverse()) {
    await cleanup()
  }
})

const vector = JSON.parse(
  readFileSync(new URL('../../../tests/fixtures/push-envelope-v1.json', import.meta.url), 'utf8')
) as { envelope: unknown }

const runNode = promisify(execFile)
const nativeClient = fileURLToPath(new URL('./client-address-request.mjs', import.meta.url))

// Launch asynchronously so the real Bun listeners keep serving while Node binds
// downstream sockets to distinct loopback IPs. No caller header supplies identity.
async function request(
  url: URL,
  localAddress = '127.0.0.1',
  options: { body?: unknown; headers?: Record<string, string | string[]>; method?: string } = {}
): Promise<Response> {
  const { stdout } = await runNode(
    'node',
    [nativeClient, JSON.stringify({ url: url.href, localAddress, ...options })],
    { timeout: 10_000, maxBuffer: 65_536 }
  )
  const result = JSON.parse(stdout) as { status: number; body: string }
  return new Response(result.body, { status: result.status })
}

function setup() {
  const fixture = new AppleFixture()
  const sent: ApnsRequest[] = []
  const now = Math.floor(Date.now() / 1000)
  const key = generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
  const apns = new ApnsTransport(
    {
      teamId: 'TESTTEAM01',
      keyId: 'TESTKEY01',
      privateKey: key.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString(),
      topic: 'com.serverbee.mobile'
    },
    (delivery) => {
      sent.push(delivery)
      return Promise.resolve({ status: 200 })
    },
    () => now
  )
  const relay = new Relay(':memory:', fixture.trust(), () => now, apns)
  cleanups.push(
    () => fixture.close(),
    () => relay.db.close()
  )
  const fetchRelay = createRelayFetch(relay, parseTrustedProxyIPs('127.0.0.1'))
  const listener = serve({ hostname: '127.0.0.1', port: 0, fetch: fetchRelay })
  cleanups.push(() => listener.stop(true))
  return { fixture, relay, sent, now, listener, fetchRelay }
}

test('proxy preserves per-client quotas, overwrites forged headers and admits the other client', async () => {
  const flow = setup()
  const peers = new Set<string>()
  const proxy = serve({
    hostname: '127.0.0.1',
    port: 0,
    async fetch(incoming, server) {
      const peer = server.requestIP(incoming)?.address
      if (!peer) {
        return new Response(null, { status: 400 })
      }
      peers.add(peer)
      const headers = new Headers(incoming.headers)
      headers.set('X-ServerBee-Client-IP', peer)
      headers.delete('host')
      return await fetch(new URL(new URL(incoming.url).pathname, flow.listener.url), {
        method: incoming.method,
        headers,
        body: incoming.body ? await incoming.arrayBuffer() : undefined
      })
    }
  })
  cleanups.push(() => proxy.stop(true))
  const attacker = '127.0.0.2'
  const legitimate = '127.0.0.3'
  const pendingResponse = await request(new URL('/v1/challenges', proxy.url), legitimate, {
    body: { action: 'attest', key_id: flow.fixture.keyId, device_token: 'a'.repeat(64), environment: 'sandbox' }
  })
  expect(pendingResponse.status).toBe(200)
  const challenge = (await pendingResponse.json()) as { challenge_id: string; client_data: string }
  const admitted = await request(new URL('/v1/attest', proxy.url), legitimate, {
    body: {
      challenge_id: challenge.challenge_id,
      proof: flow.fixture.attestation(Buffer.from(challenge.client_data, 'base64'))
    }
  })
  expect(admitted.status).toBe(200)
  const grant = (await admitted.json()) as { grant_token: string }
  for (let attempt = 0; attempt < 31; attempt += 1) {
    const response = await request(new URL('/not-found', proxy.url), attacker, {
      headers: {
        'X-ServerBee-Client-IP': [legitimate, `192.0.2.${attempt + 1}`],
        'X-Forwarded-For': `198.51.100.${attempt + 1}`,
        Forwarded: `for=203.0.113.${attempt + 1}`
      }
    })
    expect(response.status).toBe(attempt < 30 ? 404 : 429)
  }
  const headers = { authorization: `Bearer ${grant.grant_token}` }
  const inspected = await request(new URL('/v1/grants/inspect', proxy.url), legitimate, { method: 'POST', headers })
  expect(inspected.status).toBe(200)
  expect((await inspected.json()).device_token).toBe('a'.repeat(64))
  const delivered = await request(new URL('/v1/send', proxy.url), legitimate, {
    headers,
    body: {
      event_id: '11111111-1111-4111-8111-111111111111',
      expires_at: flow.now + 1800,
      envelope: vector.envelope
    }
  })
  expect(delivered.status).toBe(200)
  expect((await delivered.json()).outcome).toBe('accepted')
  expect(flow.sent).toHaveLength(1)
  expect(flow.sent[0]?.token).toBe('a'.repeat(64))
  expect(peers).toEqual(new Set([attacker, legitimate]))
  expect(flow.relay.db.query('SELECT source, count FROM request_limits ORDER BY source').all()).toEqual([
    { source: attacker, count: 31 },
    { source: legitimate, count: 4 }
  ])
}, 20_000)

test('untrusted socket ignores every spoofed forwarding header and keeps its own quota', async () => {
  const flow = setup()
  for (let attempt = 0; attempt < 31; attempt += 1) {
    const response = await request(new URL('/not-found', flow.listener.url), '127.0.0.2', {
      headers: {
        'X-ServerBee-Client-IP': `192.0.2.${attempt + 1}`,
        'X-Forwarded-For': `198.51.100.${attempt + 1}`,
        Forwarded: `for=203.0.113.${attempt + 1}`
      }
    })
    expect(response.status).toBe(attempt < 30 ? 404 : 429)
  }
  expect(flow.relay.db.query('SELECT source, count FROM request_limits').all()).toEqual([
    { source: '127.0.0.2', count: 31 }
  ])
}, 20_000)

test('trusted sockets reject missing, malformed and observable list headers before admission', async () => {
  const flow = setup()
  const invalid: (string | string[] | undefined)[] = [
    undefined,
    '',
    'unknown',
    '192.0.2.1:443',
    '[2001:db8::1]',
    'fe80::1%eth0',
    '192.0.2.1, 192.0.2.2',
    '192.0.2.1, 192.0.2.1'
  ]
  for (const value of invalid) {
    expect(
      (
        await request(new URL('/v1/challenges', flow.listener.url), '127.0.0.1', {
          headers: value === undefined ? {} : { 'X-ServerBee-Client-IP': value },
          body: { action: 'attest', key_id: flow.fixture.keyId, device_token: 'a'.repeat(64), environment: 'sandbox' }
        })
      ).status
    ).toBe(400)
  }
  expect(flow.relay.db.query('SELECT * FROM request_limits').all()).toEqual([])
  expect(flow.relay.db.query('SELECT * FROM challenges').all()).toEqual([])
  const missingPeer = await flow.fetchRelay(new Request('http://relay.test/'), { requestIP: () => null })
  expect(missingPeer.status).toBe(400)
}, 20_000)

test('documents raw duplicate-header visibility while the trusted proxy remains responsible for overwriting', async () => {
  const flow = setup()
  const probe = serve({
    hostname: '127.0.0.1',
    port: 0,
    fetch(incoming) {
      return Response.json({ visible: incoming.headers.get('X-ServerBee-Client-IP') })
    }
  })
  cleanups.push(() => probe.stop(true))
  const headers = { 'X-ServerBee-Client-IP': ['192.0.2.1', '192.0.2.2'] }
  const observed = (await (await request(probe.url, '127.0.0.1', { headers })).json()) as { visible: string }
  // Bun 1.3.4 exposes only the final field. Runtimes preserving a comma-list can
  // reject it; none may infer that a single visible value proves wire uniqueness.
  expect(['192.0.2.2', '192.0.2.1, 192.0.2.2']).toContain(observed.visible)
  const response = await request(new URL('/not-found', flow.listener.url), '127.0.0.1', { headers })
  if (normalizeIP(observed.visible)) {
    expect(response.status).toBe(404)
    expect(flow.relay.db.query('SELECT source, count FROM request_limits').all()).toEqual([
      { source: '192.0.2.2', count: 1 }
    ])
  } else {
    expect(response.status).toBe(400)
    expect(flow.relay.db.query('SELECT * FROM request_limits').all()).toEqual([])
  }
}, 20_000)

test('canonical IPv6 spellings share the same real SQLite rate-limit key', async () => {
  const flow = setup()
  const variants = ['2001:db8::a', '2001:0DB8:0:0:0:0:0:000A']
  for (let attempt = 0; attempt < 31; attempt += 1) {
    const response = await request(new URL('/not-found', flow.listener.url), '127.0.0.1', {
      headers: { 'X-ServerBee-Client-IP': variants[attempt % variants.length] }
    })
    expect(response.status).toBe(attempt < 30 ? 404 : 429)
  }
  expect(flow.relay.db.query('SELECT source, count FROM request_limits').all()).toEqual([
    { source: '2001:db8::a', count: 31 }
  ])
}, 20_000)

test('native Bun listener retains streamed body bounds for an oversized upload', async () => {
  const flow = setup()
  const response = await request(new URL('/v1/challenges', flow.listener.url), '127.0.0.1', {
    headers: { 'X-ServerBee-Client-IP': '192.0.2.1' },
    body: { oversized: 'x'.repeat(32_768) }
  })
  expect(response.status).toBe(413)
  expect(flow.relay.db.query('SELECT * FROM challenges').all()).toEqual([])
}, 20_000)

test('executable rejects missing and malformed proxy configuration before loading certificates', async () => {
  const main = fileURLToPath(new URL('../src/main.ts', import.meta.url))
  for (const value of [undefined, '', 'localhost', '127.0.0.1,']) {
    try {
      await runNode(process.execPath, [main], {
        env: value === undefined ? {} : { RELAY_TRUSTED_PROXY_IPS: value },
        timeout: 5000
      })
      throw new Error('Invalid configuration unexpectedly started')
    } catch (error) {
      const failure = error as Error & { stderr?: string; code?: number }
      expect(failure.code).toBe(1)
      expect(failure.stderr).toContain('RELAY_TRUSTED_PROXY_IPS')
      expect(failure.stderr).not.toContain('Missing APP_ATTEST_ROOT_CA')
    }
  }
})

test('native fetch seam normalizes socket peers before trust and quota selection', async () => {
  const flow = setup()
  const forwarded = new Request('http://relay.test/not-found', {
    headers: { 'X-ServerBee-Client-IP': '198.51.100.1' }
  })
  expect((await flow.fetchRelay(forwarded, { requestIP: () => ({ address: '::ffff:127.0.0.1' }) })).status).toBe(404)
  for (const address of ['::ffff:127.0.0.2', '127.0.0.2']) {
    expect((await flow.fetchRelay(forwarded, { requestIP: () => ({ address }) })).status).toBe(404)
  }
  expect(flow.relay.db.query('SELECT source, count FROM request_limits ORDER BY source').all()).toEqual([
    { source: '127.0.0.2', count: 2 },
    { source: '198.51.100.1', count: 1 }
  ])
})

test('normalizes literal IPs and rejects ambiguous address/configuration forms', () => {
  for (const value of ['::ffff:192.0.2.1', '0:0:0:0:0:ffff:c000:0201', '192.0.2.1']) {
    expect(normalizeIP(value)).toBe('192.0.2.1')
  }
  expect(normalizeIP('2001:0DB8:0:0:0:0:0:000A')).toBe('2001:db8::a')
  expect(parseTrustedProxyIPs(' 127.0.0.1, ::ffff:127.0.0.1, 0:0:0:0:0:0:0:1 ')).toEqual(new Set(['127.0.0.1', '::1']))
  for (const value of ['', 'localhost', '127.1', '2130706433', '0177.0.0.1', '127.0.0.1:80', '[::1]', 'fe80::1%lo']) {
    expect(normalizeIP(value)).toBeNull()
    expect(() => parseTrustedProxyIPs(value)).toThrow()
  }
  for (const value of ['127.0.0.0/8', '127.0.0.1,', ',127.0.0.1', '127.0.0.1,,::1']) {
    expect(() => parseTrustedProxyIPs(value)).toThrow()
  }
})
