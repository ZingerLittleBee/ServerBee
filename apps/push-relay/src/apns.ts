import { createPrivateKey, sign } from 'node:crypto'
import { connect } from 'node:http2'
import type { Environment } from './attestation'

export interface ProviderReply {
  reason?: string
  status: number
}
export interface DeliveryVerdict {
  device_invalid: boolean
  outcome: 'accepted' | 'retryable' | 'permanent' | 'expired'
  reason: string
}
export interface ApnsRequest {
  environment: Environment
  headers: Record<string, string>
  payload: string
  token: string
}
export type ApnsNetwork = (request: ApnsRequest) => Promise<ProviderReply>

/** The only replacement seam in transport tests is Apple's HTTP/2 network. */
export function appleNetwork(request: ApnsRequest): Promise<ProviderReply> {
  const host = request.environment === 'sandbox' ? 'api.sandbox.push.apple.com' : 'api.push.apple.com'
  return new Promise((resolve, reject) => {
    const client = connect(`https://${host}`)
    const timer = setTimeout(() => {
      client.destroy()
      reject(new Error('APNs timeout'))
    }, 10_000)
    client.on('error', reject)
    const stream = client.request({ ':method': 'POST', ':path': `/3/device/${request.token}`, ...request.headers })
    let status = 0
    let response = ''
    stream.on('response', (headers) => {
      status = Number(headers[':status'])
    })
    stream.on('data', (chunk: Buffer) => {
      response += chunk.toString()
      if (Buffer.byteLength(response) > 4096) {
        stream.destroy(new Error('Oversized APNs response'))
      }
    })
    stream.on('error', reject)
    stream.on('close', () => {
      clearTimeout(timer)
      client.close()
    })
    stream.on('end', () => {
      let reason: string | undefined
      try {
        reason = (JSON.parse(response) as { reason?: string }).reason
      } catch {
        /* Empty acceptance body. */
      }
      resolve({ status, reason })
    })
    stream.end(request.payload)
  })
}

export function classify(reply: ProviderReply): DeliveryVerdict {
  if (reply.status === 200) {
    return { outcome: 'accepted', reason: 'Accepted', device_invalid: false }
  }
  if (reply.status === 410 && reply.reason === 'Unregistered') {
    return { outcome: 'permanent', reason: 'Unregistered', device_invalid: true }
  }
  if (reply.status === 429 || reply.status >= 500) {
    return { outcome: 'retryable', reason: 'ProviderUnavailable', device_invalid: false }
  }
  // BadDeviceToken can mean an environment mismatch. Payload, topic, signing
  // and authorization failures must never erase a valid installation.
  return {
    outcome: 'permanent',
    reason: reply.reason === 'BadDeviceToken' ? 'DeviceOrEnvironmentMismatch' : 'ProviderConfigurationOrPayload',
    device_invalid: false
  }
}

interface SigningConfig {
  keyId: string
  privateKey: string
  teamId: string
  topic: string
}

export class ApnsTransport {
  private readonly config: SigningConfig
  private readonly network: ApnsNetwork
  private readonly clock: () => number
  private jwt?: { value: string; created: number }
  private readonly signingKey: ReturnType<typeof createPrivateKey>
  constructor(
    config: SigningConfig,
    network: ApnsNetwork = appleNetwork,
    clock: () => number = () => Math.floor(Date.now() / 1000)
  ) {
    this.config = config
    this.network = network
    this.clock = clock
    this.signingKey = createPrivateKey(config.privateKey)
    if (
      this.signingKey.asymmetricKeyType !== 'ec' ||
      this.signingKey.asymmetricKeyDetails?.namedCurve !== 'prime256v1'
    ) {
      throw new Error('APNs requires a P-256 signing key')
    }
  }
  private authorization(): string {
    const now = this.clock()
    if (!this.jwt || now - this.jwt.created >= 3000 || now < this.jwt.created) {
      const header = Buffer.from(JSON.stringify({ alg: 'ES256', kid: this.config.keyId })).toString('base64url')
      const claims = Buffer.from(JSON.stringify({ iss: this.config.teamId, iat: now })).toString('base64url')
      const input = `${header}.${claims}`
      const signature = sign('sha256', Buffer.from(input), {
        key: this.signingKey,
        dsaEncoding: 'ieee-p1363'
      }).toString('base64url')
      this.jwt = { value: `${input}.${signature}`, created: now }
    }
    return `bearer ${this.jwt.value}`
  }
  async send(
    token: string,
    environment: Environment,
    eventId: string,
    expiresAt: number,
    envelope: unknown
  ): Promise<DeliveryVerdict> {
    if (expiresAt <= this.clock()) {
      return { outcome: 'expired', reason: 'Expired', device_invalid: false }
    }
    const payload = JSON.stringify({
      aps: {
        alert: { title: 'ServerBee', body: 'Open ServerBee to view this notification.' },
        'mutable-content': 1,
        sound: 'default'
      },
      serverbee_envelope: envelope
    })
    if (Buffer.byteLength(payload) > 4096) {
      return { outcome: 'permanent', reason: 'PayloadTooLarge', device_invalid: false }
    }
    try {
      return classify(
        await this.network({
          token,
          environment,
          payload,
          headers: {
            authorization: this.authorization(),
            'apns-topic': this.config.topic,
            'apns-push-type': 'alert',
            'apns-priority': '10',
            'apns-expiration': String(expiresAt),
            'apns-id': eventId,
            'content-type': 'application/json'
          }
        })
      )
    } catch {
      return { outcome: 'retryable', reason: 'NetworkUnavailable', device_invalid: false }
    }
  }
}
