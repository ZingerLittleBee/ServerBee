import { type ApnsConfig, type ApnsFetch, ApnsTransport, type SendRequest, verdict } from './apns'
import { BodyError, cancelBody, deadline, readBytes } from './body'
import { cloudflareClientIp, WindowLimiter } from './limits'

export interface Env {
  APNS_ENVIRONMENTS?: string
  APNS_KEY_ID: string
  APNS_PRIVATE_KEY: string
  APNS_TEAM_ID: string
  APNS_TOPIC: string
}
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/
const hex64 = /^[a-f0-9]{64}$/
const nonHex = /[^a-f0-9]/
const base64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/
const keyIdPattern = /^[A-Z0-9]{10}$/
const topicPattern = /^[A-Za-z0-9.-]{1,255}$/
const lengthPattern = /^\d+$/
const MAX_REQUEST_BYTES = 8192
// Resource ceiling for variable-length APNs tokens, not a provider token size.
const MAX_DEVICE_TOKEN_CHARS = 1024
export const LIMITS = { ip: 120, target: 60, isolate: 600, entries: 4096, concurrency: 32 } as const

function object(value: unknown, keys: string[]): value is Record<string, unknown> {
  return (
    !!value &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Object.keys(value).length === keys.length &&
    keys.every((key) => Object.hasOwn(value, key))
  )
}
function canonicalBase64(value: unknown, min: number, max: number): boolean {
  if (typeof value !== 'string' || value.length > Math.ceil(max / 3) * 4 || !base64.test(value)) {
    return false
  }
  const decoded = atob(value)
  return decoded.length >= min && decoded.length <= max && btoa(decoded) === value
}
export function validate(value: unknown, now: number): SendRequest | undefined {
  if (!object(value, ['device_token', 'environment', 'event_id', 'expires_at', 'envelope'])) {
    return undefined
  }
  if (
    typeof value.device_token !== 'string' ||
    value.device_token.length < 2 ||
    value.device_token.length > MAX_DEVICE_TOKEN_CHARS ||
    value.device_token.length % 2 !== 0 ||
    nonHex.test(value.device_token) ||
    (value.environment !== 'sandbox' && value.environment !== 'production') ||
    typeof value.event_id !== 'string' ||
    !uuid.test(value.event_id) ||
    typeof value.expires_at !== 'number' ||
    !Number.isSafeInteger(value.expires_at) ||
    value.expires_at <= 0 ||
    value.expires_at > now + 1800
  ) {
    return undefined
  }
  const envelope = value.envelope
  if (
    !object(envelope, ['version', 'key_id', 'identity', 'nonce', 'ciphertext']) ||
    envelope.version !== 1 ||
    typeof envelope.key_id !== 'string' ||
    !uuid.test(envelope.key_id) ||
    typeof envelope.identity !== 'string' ||
    !hex64.test(envelope.identity) ||
    !canonicalBase64(envelope.nonce, 12, 12) ||
    !canonicalBase64(envelope.ciphertext, 16, 2070)
  ) {
    return undefined
  }
  return value as unknown as SendRequest
}

function allowedEnvironments(env: Env): string[] | undefined {
  const raw = env.APNS_ENVIRONMENTS ?? 'sandbox,production'
  if (typeof raw !== 'string' || raw.length > 64) {
    return undefined
  }
  const values = raw.split(',').map((value) => value.trim())
  return values.length >= 1 &&
    values.length <= 2 &&
    values.every((value) => value === 'sandbox' || value === 'production') &&
    new Set(values).size === values.length
    ? values
    : undefined
}
function configuration(env: Env): ApnsConfig | undefined {
  if (
    !(keyIdPattern.test(env.APNS_TEAM_ID ?? '') && keyIdPattern.test(env.APNS_KEY_ID ?? '')) ||
    typeof env.APNS_PRIVATE_KEY !== 'string' ||
    env.APNS_PRIVATE_KEY.length > 4096 ||
    typeof env.APNS_TOPIC !== 'string' ||
    !topicPattern.test(env.APNS_TOPIC)
  ) {
    return undefined
  }
  return { teamId: env.APNS_TEAM_ID, keyId: env.APNS_KEY_ID, privateKey: env.APNS_PRIVATE_KEY, topic: env.APNS_TOPIC }
}
function json(status: number, outcome: Parameters<typeof verdict>[0], reason: string, headers = {}): Response {
  return Response.json(verdict(outcome, reason), { status, headers: { 'Cache-Control': 'no-store', ...headers } })
}

async function readRequest(
  request: Request,
  now: () => number,
  environments: string[],
  timeoutMs: number
): Promise<SendRequest | Response> {
  let parsed: unknown
  try {
    const bytes = await deadline(timeoutMs, request.signal, (signal) =>
      readBytes(request.body, MAX_REQUEST_BYTES, signal)
    )
    parsed = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes))
  } catch (error) {
    if (error instanceof BodyError && error.kind === 'too_large') {
      return json(413, 'permanent', 'RequestTooLarge')
    }
    if (error instanceof BodyError && error.kind === 'timeout') {
      return json(408, 'retryable', 'RequestTimeout')
    }
    return json(400, 'permanent', 'InvalidRequest')
  }
  const body = validate(parsed, Math.floor(now() / 1000))
  if (!body) {
    return json(400, 'permanent', 'InvalidRequest')
  }
  if (!environments.includes(body.environment)) {
    return json(400, 'permanent', 'EnvironmentDisabled')
  }
  if (body.expires_at <= Math.floor(now() / 1000)) {
    return json(200, 'expired', 'Expired')
  }
  return body
}

function requestHeadersError(request: Request): Response | undefined {
  if (
    request.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== 'application/json' ||
    (request.headers.has('content-encoding') && request.headers.get('content-encoding') !== 'identity')
  ) {
    return json(415, 'permanent', 'UnsupportedMediaType')
  }
  const length = request.headers.get('content-length')
  if (length && (!lengthPattern.test(length) || Number(length) > MAX_REQUEST_BYTES)) {
    return json(413, 'permanent', 'RequestTooLarge')
  }
  return undefined
}

/** Test seams are local imports only, never bindings or client-selectable options. */
export function createRelay(
  options: {
    network?: ApnsFetch
    now?: () => number
    clientIp?: (request: Request) => string
    bodyTimeoutMs?: number
    apnsTimeoutMs?: number
  } = {}
) {
  const now = options.now ?? Date.now
  const clientIp = options.clientIp ?? cloudflareClientIp
  const ips = new WindowLimiter(LIMITS.entries)
  const targets = new WindowLimiter(LIMITS.entries)
  const global = new WindowLimiter(1)
  let active = 0
  let transport: { config: ApnsConfig; apns: ApnsTransport } | undefined
  function sender(config: ApnsConfig): ApnsTransport {
    if (
      !transport ||
      Object.keys(config).some((key) => config[key as keyof ApnsConfig] !== transport?.config[key as keyof ApnsConfig])
    ) {
      transport = {
        config,
        apns: new ApnsTransport(config, options.network, () => Math.floor(now() / 1000), options.apnsTimeoutMs)
      }
    }
    return transport.apns
  }
  return {
    async fetch(request: Request, env: Env): Promise<Response> {
      const reject = (status: number, outcome: Parameters<typeof verdict>[0], reason: string, headers = {}) => {
        cancelBody(request.body)
        return json(status, outcome, reason, headers)
      }
      if (new URL(request.url).pathname !== '/v1/send') {
        return reject(404, 'permanent', 'NotFound')
      }
      if (request.method !== 'POST') {
        return reject(405, 'permanent', 'MethodNotAllowed', { Allow: 'POST' })
      }
      if (active >= LIMITS.concurrency) {
        return reject(503, 'retryable', 'RelayBusy', { 'Retry-After': '1' })
      }
      if (!(global.allow('isolate', LIMITS.isolate, now()) && ips.allow(clientIp(request), LIMITS.ip, now()))) {
        return reject(429, 'retryable', 'RateLimited', { 'Retry-After': '60' })
      }
      const config = configuration(env)
      const environments = allowedEnvironments(env)
      if (!(config && environments)) {
        return reject(503, 'permanent', 'RelayNotConfigured')
      }
      const headerError = requestHeadersError(request)
      if (headerError) {
        cancelBody(request.body)
        return headerError
      }
      active += 1
      try {
        const body = await readRequest(request, now, environments, options.bodyTimeoutMs ?? 5000)
        if (body instanceof Response) {
          return body
        }
        if (!targets.allow(`${body.environment}:${body.device_token}`, LIMITS.target, now())) {
          return json(429, 'retryable', 'RateLimited', { 'Retry-After': '60' })
        }
        return Response.json(await sender(config).send(body, request.signal), {
          headers: { 'Cache-Control': 'no-store' }
        })
      } finally {
        active -= 1
      }
    }
  }
}

export default createRelay()
