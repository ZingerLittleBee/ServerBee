import { env, exports } from 'cloudflare:workers'
import { expect, test, vi } from 'vitest'
import vector from '../../../tests/fixtures/push-envelope-v1.json'
import { ApnsTransport, classify, type SendRequest } from '../src/apns'
import { cloudflareClientIp, WindowLimiter } from '../src/limits'
import { createRelay, type Env, LIMITS } from '../src/relay'

const bindings = env as unknown as Env
const encoder = new TextEncoder()
const now = 2_000_000_000
function body(): SendRequest {
  return {
    device_token: 'a'.repeat(64),
    environment: 'sandbox',
    event_id: '11111111-1111-4111-8111-111111111111',
    expires_at: now + 1800,
    envelope: { ...vector.envelope, version: 1 }
  }
}
function request(value: unknown = body(), headers: Record<string, string> = {}): Request {
  return new Request('https://relay.test/v1/send', {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...headers },
    body: JSON.stringify(value)
  })
}
function streamed(stream: ReadableStream<Uint8Array>, headers: Record<string, string> = {}): Request {
  return new Request('https://relay.test/v1/send', {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...headers },
    body: stream
  })
}
function setup(options: Parameters<typeof createRelay>[0] = {}) {
  const network = vi.fn(async () => new Response(null, { status: 200 }))
  return { network, relay: createRelay({ now: () => now * 1000, network, ...options }) }
}

test('runs the configured default Worker and real outbound fetch boundary in workerd', async () => {
  expect(bindings.APNS_TOPIC).toBe('app.serverbee')
  const value = body()
  value.expires_at = Math.floor(Date.now() / 1000) + 1800
  const response = await (exports as unknown as { default: Fetcher }).default.fetch(request(value))
  expect(await response.json()).toEqual({ outcome: 'accepted', reason: 'Accepted', device_invalid: false })
})

test('forwards the unchanged cross-language envelope with fixed alert, hosts, topic and APNs headers', async () => {
  const calls: { url: string; init: RequestInit }[] = []
  const relay = createRelay({
    now: () => now * 1000,
    network: (url, init) => {
      calls.push({ url, init })
      return Promise.resolve(new Response(null))
    }
  })
  for (const environment of ['sandbox', 'production'] as const) {
    const value = { ...body(), environment }
    const response = await relay.fetch(request(value), bindings)
    expect(await response.json()).toEqual({ outcome: 'accepted', reason: 'Accepted', device_invalid: false })
    expect(response.headers.get('cache-control')).toBe('no-store')
    const call = calls.at(-1)
    if (!call) {
      throw new Error('Missing APNs call')
    }
    expect(call.url).toBe(
      `https://${environment === 'sandbox' ? 'api.sandbox.push.apple.com' : 'api.push.apple.com'}/3/device/${value.device_token}`
    )
    expect(call.init.redirect).toBe('manual')
    const headers = new Headers(call.init.headers)
    expect(headers.get('apns-topic')).toBe(bindings.APNS_TOPIC)
    expect(headers.get('apns-push-type')).toBe('alert')
    expect(headers.get('apns-priority')).toBe('10')
    expect(headers.get('apns-expiration')).toBe(String(value.expires_at))
    expect(headers.get('apns-id')).toBe(value.event_id)
    const payload = JSON.parse(call.init.body as string)
    expect(payload.serverbee_envelope).toEqual(vector.envelope)
    expect(payload.aps).toEqual({
      alert: { title: 'ServerBee', body: 'Open ServerBee to view this notification.' },
      'mutable-content': 1,
      sound: 'default'
    })
    expect(call.init.body).not.toContain('content_key')
    expect(call.init.body).not.toContain('deployment_id')
  }
  expect(new Headers(calls[0].init.headers).get('authorization')).toBe(
    new Headers(calls[1].init.headers).get('authorization')
  )
})

test('WebCrypto ES256 JWT verifies, caches concurrently, refreshes at 50 minutes and on clock rollback', async () => {
  let clock = now
  const pair = (await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, [
    'sign',
    'verify'
  ])) as CryptoKeyPair
  const der = new Uint8Array((await crypto.subtle.exportKey('pkcs8', pair.privateKey)) as ArrayBuffer)
  const pem = `-----BEGIN PRIVATE KEY-----\n${btoa(String.fromCharCode(...der))}\n-----END PRIVATE KEY-----`
  const tokens: string[] = []
  const apns = new ApnsTransport(
    { teamId: 'TESTTEAM01', keyId: 'TESTKEY001', topic: bindings.APNS_TOPIC, privateKey: pem },
    (_url, init) => {
      tokens.push(new Headers(init.headers).get('authorization') ?? '')
      return Promise.resolve(new Response(null))
    },
    () => clock
  )
  await Promise.all(Array.from({ length: 8 }, () => apns.send(body())))
  expect(new Set(tokens).size).toBe(1)
  const [header, claims, signature] = tokens[0].slice(7).split('.')
  expect(JSON.parse(atob(header))).toEqual({ alg: 'ES256', kid: 'TESTKEY001' })
  expect(JSON.parse(atob(claims))).toEqual({ iss: 'TESTTEAM01', iat: now })
  const raw = Uint8Array.from(atob(signature.replaceAll('-', '+').replaceAll('_', '/')), (char) => char.charCodeAt(0))
  expect(raw.length).toBe(64)
  expect(
    await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      pair.publicKey,
      raw,
      encoder.encode(`${header}.${claims}`)
    )
  ).toBe(true)
  clock += 2999
  await apns.send({ ...body(), expires_at: clock + 1800 })
  expect(tokens.at(-1)).toBe(tokens[0])
  clock += 1
  await apns.send({ ...body(), expires_at: clock + 1800 })
  expect(tokens.at(-1)).not.toBe(tokens[0])
  const refreshed = tokens.at(-1)
  clock -= 100
  await apns.send({ ...body(), expires_at: clock + 1800 })
  expect(tokens.at(-1)).not.toBe(refreshed)
})

test.each([
  [
    'missing token',
    (v: Record<string, unknown>) => {
      v.device_token = undefined
    }
  ],
  [
    'extra authorization',
    (v: Record<string, unknown>) => {
      v.grant_token = 'old'
    }
  ],
  [
    'extra topic',
    (v: Record<string, unknown>) => {
      v.topic = 'attacker'
    }
  ],
  [
    'bad token',
    (v: Record<string, unknown>) => {
      v.device_token = 'A'.repeat(64)
    }
  ],
  [
    'bad environment',
    (v: Record<string, unknown>) => {
      v.environment = 'https://attacker'
    }
  ],
  [
    'bad event',
    (v: Record<string, unknown>) => {
      v.event_id = '../bad'
    }
  ],
  [
    'fraction expiry',
    (v: Record<string, unknown>) => {
      v.expires_at = now + 0.5
    }
  ],
  [
    'far future',
    (v: Record<string, unknown>) => {
      v.expires_at = now + 1801
    }
  ],
  [
    'negative expiry',
    (v: Record<string, unknown>) => {
      v.expires_at = -1
    }
  ],
  [
    'envelope array',
    (v: Record<string, unknown>) => {
      v.envelope = []
    }
  ]
])('rejects %s before APNs', async (_name, mutate) => {
  const { relay, network } = setup()
  const value = body() as unknown as Record<string, unknown>
  mutate(value)
  expect((await relay.fetch(request(value), bindings)).status).toBe(400)
  expect(network).not.toHaveBeenCalled()
})

test.each([
  { version: 2 },
  { key_id: 'bad' },
  { identity: 'x'.repeat(64) },
  { nonce: 'ab==' },
  { ciphertext: 'YQ==' },
  { ciphertext: `${'A'.repeat(2764)}` },
  { ciphertext: 'A'.repeat(23) },
  { ciphertext: `${'A'.repeat(22)}B=` },
  { ciphertext: `${'A'.repeat(24)}\n` },
  { content_key: 'secret' }
])('strict envelope validation: %j', async (change) => {
  const { relay, network } = setup()
  const value = body()
  Object.assign(value.envelope, change)
  expect((await relay.fetch(request(value), bindings)).status).toBe(400)
  expect(network).not.toHaveBeenCalled()
})

test('expired envelope never reaches APNs; upload time cannot extend TTL', async () => {
  const { relay, network } = setup()
  expect(await (await relay.fetch(request({ ...body(), expires_at: now }), bindings)).json()).toMatchObject({
    outcome: 'expired'
  })
  expect(network).not.toHaveBeenCalled()
  let time = now * 1000
  const changing = createRelay({ now: () => time, network })
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoder.encode(JSON.stringify({ ...body(), expires_at: now + 1 })))
      time += 2000
      controller.close()
    }
  })
  expect(await (await changing.fetch(streamed(stream), bindings)).json()).toMatchObject({ outcome: 'expired' })
  expect(network).not.toHaveBeenCalled()
})

test('only POST /v1/send is exposed; old admission endpoints no longer exist', async () => {
  const { relay } = setup()
  for (const path of ['/v1/challenges', '/v1/attest', '/v1/renew', '/v1/revoke', '/v1/grants/inspect', '/']) {
    expect((await relay.fetch(new Request(`https://relay.test${path}`, { method: 'POST' }), bindings)).status).toBe(404)
  }
  expect((await relay.fetch(new Request('https://relay.test/v1/send'), bindings)).status).toBe(405)
})

test('bounds a streaming upload before JSON decoding even with a lying Content-Length', async () => {
  const { relay, network } = setup()
  let cancelled = false
  let reads = 0
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      reads += 1
      controller.enqueue(new Uint8Array(4096))
    },
    cancel() {
      cancelled = true
      return new Promise<void>(() => undefined)
    }
  })
  const response = await relay.fetch(streamed(stream, { 'content-length': '1' }), bindings)
  expect(response.status).toBe(413)
  expect(cancelled).toBe(true)
  expect(reads).toBeLessThanOrEqual(4)
  expect(network).not.toHaveBeenCalled()
})

test('upload deadline cancels stalled streams without waiting for hostile cancel', async () => {
  const { relay, network } = setup({ bodyTimeoutMs: 15 })
  let cancelled = false
  const stream = new ReadableStream<Uint8Array>({
    cancel() {
      cancelled = true
      return new Promise<void>(() => undefined)
    }
  })
  const response = await relay.fetch(streamed(stream), bindings)
  expect(response.status).toBe(408)
  expect(cancelled).toBe(true)
  expect(network).not.toHaveBeenCalled()
  expect((await relay.fetch(request(), bindings)).status).toBe(200)
})

test('caller cancellation stops an upload and releases its slot', async () => {
  const { relay } = setup({ bodyTimeoutMs: 500 })
  const controller = new AbortController()
  let cancelled = false
  const stream = new ReadableStream<Uint8Array>({
    cancel() {
      cancelled = true
    }
  })
  const pending = relay.fetch(
    new Request('https://relay.test/v1/send', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: stream,
      signal: controller.signal
    }),
    bindings
  )
  controller.abort()
  expect((await pending).status).toBe(408)
  expect(cancelled).toBe(true)
})

test.each([
  ['application/json', '{'],
  ['text/plain', '{}'],
  ['application/json', '[]']
])('rejects malformed JSON/media type %s %s', async (contentType, raw) => {
  const { relay, network } = setup()
  const response = await relay.fetch(
    new Request('https://relay.test/v1/send', { method: 'POST', headers: { 'content-type': contentType }, body: raw }),
    bindings
  )
  expect(response.status).toBe(contentType === 'text/plain' ? 415 : 400)
  expect(network).not.toHaveBeenCalled()
})

test('limits concurrency before reading additional uploads', async () => {
  const { relay } = setup({ bodyTimeoutMs: 1000 })
  const controllers: ReadableStreamDefaultController<Uint8Array>[] = []
  const pending = Array.from({ length: LIMITS.concurrency }, () =>
    relay.fetch(
      streamed(
        new ReadableStream<Uint8Array>({
          start(c) {
            controllers.push(c)
          }
        })
      ),
      bindings
    )
  )
  let cancelled = false
  const extra = new ReadableStream<Uint8Array>({
    cancel() {
      cancelled = true
    }
  })
  expect((await relay.fetch(streamed(extra), bindings)).status).toBe(503)
  expect(cancelled).toBe(true)
  for (const controller of controllers) {
    controller.close()
  }
  await Promise.all(pending)
  expect((await relay.fetch(request(), bindings)).status).toBe(200)
})

test('map cardinality is a hard cap even when all keys are live; old entries recover', () => {
  const limiter = new WindowLimiter(3)
  for (let i = 0; i < 10_000; i++) {
    expect(limiter.allow(String(i), 2, 1000)).toBe(i < 3)
  }
  expect(limiter.size).toBe(3)
  expect(limiter.allow('0', 2, 1000)).toBe(true)
  expect(limiter.allow('0', 2, 1000)).toBe(false)
  expect(limiter.allow('new', 2, 61_000)).toBe(true)
  expect(limiter.size).toBe(1)
  expect(limiter.allow('new', 2, 0)).toBe(true)
})

test('trusts only Cloudflare ingress IP; missing headers share the unknown bucket', () => {
  expect(cloudflareClientIp(request({}, { 'x-forwarded-for': '192.0.2.1', 'x-real-ip': '192.0.2.2' }))).toBe('unknown')
  expect(cloudflareClientIp(request({}, { 'cf-connecting-ip': '2001:db8::1' }))).toBe('2001:db8::1')
  expect(cloudflareClientIp(request({}, { 'cf-connecting-ip': 'x'.repeat(100) }))).toBe('unknown')
})

test('per-IP, per-target and isolate-global caps fail closed and recover in a new window', async () => {
  let time = now * 1000
  const { relay, network } = setup({ now: () => time })
  for (let i = 0; i < LIMITS.target; i++) {
    expect((await relay.fetch(request(body(), { 'cf-connecting-ip': `192.0.2.${i}` }), bindings)).status).toBe(200)
  }
  expect((await relay.fetch(request(body(), { 'cf-connecting-ip': '198.51.100.1' }), bindings)).status).toBe(429)
  time += 60_000
  expect((await relay.fetch(request(), bindings)).status).toBe(200)
  expect(network).toHaveBeenCalledTimes(LIMITS.target + 1)
  const ip = setup()
  for (let i = 0; i < LIMITS.ip; i++) {
    expect((await ip.relay.fetch(request({ bad: true }), bindings)).status).toBe(400)
  }
  expect((await ip.relay.fetch(request(), bindings)).status).toBe(429)
  const global = setup()
  for (let i = 0; i < LIMITS.isolate; i++) {
    expect(
      (await global.relay.fetch(request({ bad: true }, { 'cf-connecting-ip': `192.0.2.${i}` }), bindings)).status
    ).toBe(400)
  }
  expect((await global.relay.fetch(request({}, { 'cf-connecting-ip': '198.51.100.1' }), bindings)).status).toBe(429)
})

test.each([
  [200, undefined, 'accepted', false],
  [410, 'Unregistered', 'permanent', true],
  [400, 'Unregistered', 'permanent', false],
  [410, 'BadDeviceToken', 'permanent', false],
  [400, 'BadDeviceToken', 'permanent', false],
  [403, 'ExpiredProviderToken', 'permanent', false],
  [429, undefined, 'retryable', false],
  [500, undefined, 'retryable', false],
  [503, undefined, 'retryable', false],
  [200.5, undefined, 'retryable', false],
  [0, undefined, 'retryable', false]
])('classifies APNs %s %s without wrongly deleting the device', (status, reason, outcome, invalid) => {
  expect(classify(status as number, reason as string | undefined)).toMatchObject({ outcome, device_invalid: invalid })
})

test.each([
  [410, '{"reason":"Unregistered"}', 'permanent', true],
  [400, '{"reason":"BadDeviceToken"}', 'permanent', false],
  [429, '{"reason":"TooManyRequests"}', 'retryable', false],
  [500, '', 'retryable', false],
  [200, 'contradiction', 'retryable', false],
  [410, '{not json', 'retryable', false],
  [302, '', 'permanent', false]
])('real fetch pipeline classifies %s response', async (status, raw, outcome, invalid) => {
  const { relay } = setup({ network: async () => new Response((raw as string) || null, { status: status as number }) })
  expect(await (await relay.fetch(request(), bindings)).json()).toMatchObject({ outcome, device_invalid: invalid })
})

test('provider response stream cap cancels oversized data before parsing', async () => {
  let cancelled = false
  let count = 0
  const { relay } = setup({
    network: async () =>
      new Response(
        new ReadableStream<Uint8Array>({
          pull(c) {
            count += 1
            c.enqueue(new Uint8Array(3000))
          },
          cancel() {
            cancelled = true
          }
        }),
        { status: 410 }
      )
  })
  expect(await (await relay.fetch(request(), bindings)).json()).toMatchObject({
    outcome: 'retryable',
    device_invalid: false
  })
  expect(cancelled).toBe(true)
  expect(count).toBeLessThanOrEqual(3)
})

test('provider header timeout aborts fetch; late response is cancelled', async () => {
  let signal: AbortSignal | undefined
  let finish: ((response: Response) => void) | undefined
  let cancelled = false
  const { relay } = setup({
    apnsTimeoutMs: 15,
    network: (_url, init) => {
      signal = init.signal as AbortSignal
      return new Promise((resolve) => {
        finish = resolve
      })
    }
  })
  expect(await (await relay.fetch(request(), bindings)).json()).toMatchObject({ outcome: 'retryable' })
  expect(signal?.aborted).toBe(true)
  finish?.(
    new Response(
      new ReadableStream<Uint8Array>({
        cancel() {
          cancelled = true
        }
      })
    )
  )
  await new Promise((resolve) => setTimeout(resolve, 1))
  expect(cancelled).toBe(true)
})

test('provider body timeout cancels the stream and releases concurrency', async () => {
  let cancelled = false
  const { relay } = setup({
    apnsTimeoutMs: 15,
    network: async () =>
      new Response(
        new ReadableStream<Uint8Array>({
          cancel() {
            cancelled = true
          }
        }),
        { status: 410 }
      )
  })
  expect(await (await relay.fetch(request(), bindings)).json()).toMatchObject({
    outcome: 'retryable',
    device_invalid: false
  })
  expect(cancelled).toBe(true)
})

test('invalid configuration/signing key is sanitized and never fetches APNs', async () => {
  const { relay, network } = setup()
  expect((await relay.fetch(request(), { ...bindings, APNS_TOPIC: 'bad\nheader' })).status).toBe(503)
  expect(await (await relay.fetch(request(), { ...bindings, APNS_PRIVATE_KEY: 'bad key' })).json()).toMatchObject({
    outcome: 'permanent',
    reason: 'RelayNotConfigured'
  })
  expect(network).not.toHaveBeenCalled()
})

test('deployment environment allowlist is optional, strict and never caller-controlled', async () => {
  const { relay, network } = setup()
  expect((await relay.fetch(request(), { ...bindings, APNS_ENVIRONMENTS: 'production' })).status).toBe(400)
  expect(network).not.toHaveBeenCalled()
  for (const value of ['', 'sandbox,other', 'sandbox,sandbox', 'sandbox,production,', 'x'.repeat(100)]) {
    expect((await relay.fetch(request(), { ...bindings, APNS_ENVIRONMENTS: value })).status).toBe(503)
  }
  expect((await relay.fetch(request(), { ...bindings, APNS_ENVIRONMENTS: undefined })).status).toBe(200)
})

test('accepts the exact upload byte boundary and refuses one byte more', async () => {
  const { relay, network } = setup()
  const raw = JSON.stringify(body())
  const padded = raw.padEnd(8192, ' ')
  const response = await relay.fetch(
    new Request('https://relay.test/v1/send', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: padded
    }),
    bindings
  )
  expect(response.status).toBe(200)
  const oversized = await relay.fetch(
    new Request('https://relay.test/v1/send', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: `${padded} `
    }),
    bindings
  )
  expect(oversized.status).toBe(413)
  expect(network).toHaveBeenCalledTimes(1)
})

test('rejects malformed UTF-8 and compressed bodies before the provider', async () => {
  const { relay, network } = setup()
  const bytes = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new Uint8Array([0xff]))
      c.close()
    }
  })
  expect((await relay.fetch(streamed(bytes), bindings)).status).toBe(400)
  expect((await relay.fetch(request(body(), { 'content-encoding': 'gzip' }), bindings)).status).toBe(415)
  expect(network).not.toHaveBeenCalled()
})

test('caller disconnect during APNs fetch aborts the provider request', async () => {
  const controller = new AbortController()
  let providerSignal: AbortSignal | undefined
  const { relay } = setup({
    network: (_url, init) => {
      providerSignal = init.signal as AbortSignal
      controller.abort()
      return Promise.reject(new Error('aborted provider'))
    }
  })
  const value = new Request(request(), { signal: controller.signal })
  expect(await (await relay.fetch(value, bindings)).json()).toMatchObject({
    outcome: 'retryable',
    device_invalid: false
  })
  expect(providerSignal?.aborted).toBe(true)
})
