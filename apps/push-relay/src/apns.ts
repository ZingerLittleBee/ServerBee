import { cancelBody, deadline, readBytes } from './body'

export type Environment = 'sandbox' | 'production'
export interface DeliveryVerdict {
  device_invalid: boolean
  outcome: 'accepted' | 'retryable' | 'permanent' | 'expired'
  reason: string
}
export interface ApnsConfig {
  keyId: string
  privateKey: string
  teamId: string
  topic: string
}
export type ApnsFetch = (url: string, init: RequestInit) => Promise<Response>
export interface Envelope {
  ciphertext: string
  identity: string
  key_id: string
  nonce: string
  version: 1
}
export interface SendRequest {
  device_token: string
  envelope: Envelope
  environment: Environment
  event_id: string
  expires_at: number
}

const encoder = new TextEncoder()
const hosts: Record<Environment, string> = {
  sandbox: 'https://api.sandbox.push.apple.com',
  production: 'https://api.push.apple.com'
}
export function verdict(outcome: DeliveryVerdict['outcome'], reason: string): DeliveryVerdict {
  return { outcome, reason, device_invalid: false }
}
export function classify(status: number, reason?: string): DeliveryVerdict {
  if (!Number.isInteger(status) || status < 200 || status > 599) {
    return verdict('retryable', 'NetworkUnavailable')
  }
  if (status === 200) {
    return verdict('accepted', 'Accepted')
  }
  if (status === 410 && reason === 'Unregistered') {
    return { outcome: 'permanent', reason: 'Unregistered', device_invalid: true }
  }
  if (status === 429 || status >= 500) {
    return verdict('retryable', 'ProviderUnavailable')
  }
  return verdict(
    'permanent',
    reason === 'BadDeviceToken' ? 'DeviceOrEnvironmentMismatch' : 'ProviderConfigurationOrPayload'
  )
}

function base64url(bytes: Uint8Array): string {
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '')
}

/** One bounded cache and single-flight signing operation per configured transport. */
export class ApnsTransport {
  private key?: Promise<CryptoKey>
  private jwt?: { value: string; created: number }
  private signing?: Promise<string>

  private readonly config: ApnsConfig
  private readonly network: ApnsFetch
  private readonly clock: () => number
  private readonly timeoutMs: number

  constructor(
    config: ApnsConfig,
    network: ApnsFetch = (url, init) => fetch(url, init),
    clock: () => number = () => Math.floor(Date.now() / 1000),
    timeoutMs = 10_000
  ) {
    this.config = config
    this.network = network
    this.clock = clock
    this.timeoutMs = timeoutMs
  }

  private async authorization(): Promise<string> {
    const now = this.clock()
    if (this.jwt && now >= this.jwt.created && now - this.jwt.created < 3000) {
      return this.jwt.value
    }
    if (this.signing) {
      return this.signing
    }
    this.signing = (async () => {
      if (!this.key) {
        const pem = this.config.privateKey.trim()
        if (!(pem.startsWith('-----BEGIN PRIVATE KEY-----') && pem.endsWith('-----END PRIVATE KEY-----'))) {
          throw new Error('Invalid signing key')
        }
        const binary = atob(pem.slice(27, -25).replace(/\s/g, ''))
        const der = Uint8Array.from(binary, (character) => character.charCodeAt(0))
        this.key = crypto.subtle.importKey('pkcs8', der, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign'])
      }
      const header = base64url(encoder.encode(JSON.stringify({ alg: 'ES256', kid: this.config.keyId })))
      const claims = base64url(encoder.encode(JSON.stringify({ iss: this.config.teamId, iat: now })))
      const input = `${header}.${claims}`
      const signature = await crypto.subtle.sign(
        { name: 'ECDSA', hash: 'SHA-256' },
        await this.key,
        encoder.encode(input)
      )
      const value = `bearer ${input}.${base64url(new Uint8Array(signature))}`
      this.jwt = { value, created: now }
      return value
    })()
    try {
      return await this.signing
    } finally {
      this.signing = undefined
    }
  }

  async send(request: SendRequest, external?: AbortSignal): Promise<DeliveryVerdict> {
    if (request.expires_at <= this.clock()) {
      return verdict('expired', 'Expired')
    }
    const payload = JSON.stringify({
      aps: {
        alert: { title: 'ServerBee', body: 'Open ServerBee to view this notification.' },
        'mutable-content': 1,
        sound: 'default'
      },
      serverbee_envelope: request.envelope
    })
    if (encoder.encode(payload).byteLength > 4096) {
      return verdict('permanent', 'PayloadTooLarge')
    }
    try {
      return await deadline(this.timeoutMs, external, async (signal) => {
        let authorization: string
        try {
          authorization = await this.authorization()
        } catch {
          return verdict('permanent', 'RelayNotConfigured')
        }
        if (signal.aborted) {
          return verdict('retryable', 'NetworkUnavailable')
        }
        // Upload/signing may consume the remaining TTL; recheck before forwarding.
        if (request.expires_at <= this.clock()) {
          return verdict('expired', 'Expired')
        }
        const response = await this.network(`${hosts[request.environment]}/3/device/${request.device_token}`, {
          method: 'POST',
          redirect: 'manual',
          signal,
          headers: {
            authorization,
            'apns-topic': this.config.topic,
            'apns-push-type': 'alert',
            'apns-priority': '10',
            'apns-expiration': String(request.expires_at),
            'apns-id': request.event_id,
            'content-type': 'application/json'
          },
          body: payload
        })
        if (signal.aborted) {
          cancelBody(response.body)
          return verdict('retryable', 'NetworkUnavailable')
        }
        const bytes = await readBytes(response.body, response.status === 200 ? 0 : 4096, signal)
        let reason: string | undefined
        if (bytes.byteLength > 0) {
          const body: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes))
          if (body && typeof body === 'object' && 'reason' in body && typeof body.reason === 'string') {
            reason = body.reason
          }
        }
        return classify(response.status, reason)
      })
    } catch {
      // Never expose or log network errors: they can contain tokens or keys.
      return verdict('retryable', 'NetworkUnavailable')
    }
  }
}
