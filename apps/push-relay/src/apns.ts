import { createPrivateKey, sign } from 'node:crypto'
import { type ClientHttp2Session, type ClientHttp2Stream, connect, constants } from 'node:http2'
import type { Socket } from 'node:net'
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

interface AppleSession {
  client: ClientHttp2Session
  environment: Environment
  pending: Map<ClientHttp2Stream, (error: Error) => void>
  socket?: Socket
}

const STATUS_PATTERN = /^[2-5]\d{2}$/

function validStatus(status: number): boolean {
  return Number.isInteger(status) && status >= 200 && status <= 599
}

/** Owns at most one accepting and one draining HTTP/2 session per environment. */
export class AppleNetwork {
  private readonly sessions = new Map<Environment, AppleSession>()
  private readonly draining = new Map<Environment, AppleSession>()
  private closed = false
  private readonly open: (environment: Environment) => ClientHttp2Session
  private readonly timeoutMs: number

  constructor(
    open: (environment: Environment) => ClientHttp2Session = (environment) =>
      connect(`https://${environment === 'sandbox' ? 'api.sandbox.push.apple.com' : 'api.push.apple.com'}`),
    timeoutMs = 10_000
  ) {
    this.open = open
    this.timeoutMs = timeoutMs
  }

  private forget(session: AppleSession): void {
    for (const sessions of [this.sessions, this.draining]) {
      if (sessions.get(session.environment) === session) {
        sessions.delete(session.environment)
      }
    }
  }

  private destroy(session: AppleSession, error: Error): void {
    this.forget(session)
    for (const fail of session.pending.values()) {
      fail(error)
    }
    session.client.destroy()
    // A gracefully closed HTTP/2 session can leave a half-open TCP socket.
    // The connect event supplies the real socket, not session.socket's proxy.
    session.socket?.destroy()
  }

  private retire(session: AppleSession): void {
    this.forget(session)
    const previous = this.draining.get(session.environment)
    if (previous && previous !== session) {
      this.destroy(previous, new Error('APNs session replaced'))
    }
    this.draining.set(session.environment, session)
    // Stop assigning streams here; explicit client.close() aborts live streams
    // in Bun. Each remaining stream has a deadline and the last one destroys it.
  }

  private session(environment: Environment): AppleSession {
    if (this.closed) {
      throw new Error('APNs transport closed')
    }
    const existing = this.sessions.get(environment)
    if (existing && !existing.client.closed && !existing.client.destroyed) {
      return existing
    }
    if (existing) {
      this.destroy(existing, new Error('APNs session closed'))
    }
    const client = this.open(environment)
    const session: AppleSession = { client, environment, pending: new Map() }
    this.sessions.set(environment, session)
    client.on('error', (error) => this.destroy(session, error))
    client.on('close', () => this.destroy(session, new Error('APNs session closed')))
    client.on('connect', (_client, socket: Socket) => {
      session.socket = socket
      if (client.destroyed) {
        socket.destroy()
      }
    })
    client.on('goaway', (code, lastStreamId) => {
      // Repeated GOAWAY rotations cannot accumulate retiring connections.
      this.retire(session)
      for (const [stream, fail] of session.pending) {
        if (code !== constants.NGHTTP2_NO_ERROR || stream.id === undefined || stream.id > lastStreamId) {
          fail(new Error('APNs GOAWAY'))
        }
      }
      // Accepted streams can finish; their own deadlines bound the drain.
      if (session.pending.size === 0) {
        this.destroy(session, new Error('APNs session drained'))
      }
    })
    return session
  }

  /** The only replacement seam in transport tests remains Apple's network. */
  readonly send: ApnsNetwork = (request) =>
    new Promise((resolve, reject) => {
      const session = this.session(request.environment)
      let stream: ClientHttp2Stream
      try {
        stream = session.client.request({
          ':method': 'POST',
          ':path': `/3/device/${request.token}`,
          ...request.headers
        })
      } catch (error) {
        if (session.pending.size === 0) {
          this.destroy(session, new Error('APNs request failed'))
        }
        reject(error)
        return
      }
      let settled = false
      let status = 0
      let response = ''
      let responseBytes = 0
      const finish = (error?: Error, reply?: ProviderReply) => {
        if (settled) {
          return
        }
        settled = true
        clearTimeout(timer)
        session.pending.delete(stream)
        if (error) {
          reject(error)
          stream.close(constants.NGHTTP2_CANCEL)
          stream.destroy()
        } else if (reply) {
          resolve(reply)
        }
        if (
          session.pending.size === 0 &&
          (this.draining.get(session.environment) === session || session.client.connecting)
        ) {
          this.destroy(session, new Error('APNs session drained'))
        }
      }
      const timer = setTimeout(() => {
        // A blackholed connection must not trap all future retries, but healthy
        // sibling streams may still finish on the retiring connection.
        this.retire(session)
        finish(new Error('APNs timeout'))
      }, this.timeoutMs)
      session.pending.set(stream, (error) => finish(error))
      stream.on('response', (headers, _flags, rawHeaders) => {
        status = Number(headers[':status'])
        const statuses = rawHeaders.filter((name, index) => index % 2 === 0 && name === ':status')
        const rawStatus = rawHeaders[rawHeaders.indexOf(':status') + 1]
        // Bun normalizes malformed values such as 200.5 to 200 in headers.
        if (
          statuses.length !== 1 ||
          !STATUS_PATTERN.test(rawStatus) ||
          Number(rawStatus) !== status ||
          !validStatus(status)
        ) {
          finish(new Error('Invalid APNs response status'))
        }
      })
      stream.on('trailers', (_headers, _flags, rawHeaders) => {
        if (rawHeaders.some((name, index) => index % 2 === 0 && name.startsWith(':'))) {
          finish(new Error('Invalid APNs response trailers'))
        }
      })
      stream.on('data', (chunk: Buffer) => {
        if (settled) {
          return
        }
        responseBytes += chunk.length
        // Apple specifies an empty body for success; reject contradictory data.
        if (status === 200 && responseBytes > 0) {
          finish(new Error('Invalid APNs success body'))
          return
        }
        if (responseBytes > 4096) {
          finish(new Error('Oversized APNs response'))
          return
        }
        response += chunk.toString()
      })
      // Keep error/close listeners through destruction to consume late events.
      stream.on('error', (error) => finish(error))
      stream.on('aborted', () => finish(new Error('APNs stream aborted')))
      stream.on('close', () => finish(new Error('APNs stream closed before response completed')))
      stream.on('end', () => {
        if (!validStatus(status)) {
          finish(new Error('Missing APNs response status'))
          return
        }
        let reason: string | undefined
        try {
          const body: unknown = JSON.parse(response)
          if (body && typeof body === 'object' && 'reason' in body && typeof body.reason === 'string') {
            reason = body.reason
          }
        } catch {
          /* Empty acceptance body. */
        }
        finish(undefined, { status, reason })
      })
      try {
        stream.end(request.payload)
      } catch {
        finish(new Error('APNs request failed'))
      }
    })

  /** Immediate, idempotent shutdown also settles requests waiting for headers. */
  close(): void {
    this.closed = true
    for (const session of [...this.sessions.values(), ...this.draining.values()]) {
      this.destroy(session, new Error('APNs transport closed'))
    }
  }
}

export function classify(reply: ProviderReply): DeliveryVerdict {
  if (!validStatus(reply.status)) {
    return { outcome: 'retryable', reason: 'NetworkUnavailable', device_invalid: false }
  }
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
  private readonly apple?: AppleNetwork
  private readonly clock: () => number
  private jwt?: { value: string; created: number }
  private readonly signingKey: ReturnType<typeof createPrivateKey>
  constructor(config: SigningConfig, network?: ApnsNetwork, clock: () => number = () => Math.floor(Date.now() / 1000)) {
    this.config = config
    this.clock = clock
    this.signingKey = createPrivateKey(config.privateKey)
    if (
      this.signingKey.asymmetricKeyType !== 'ec' ||
      this.signingKey.asymmetricKeyDetails?.namedCurve !== 'prime256v1'
    ) {
      throw new Error('APNs requires a P-256 signing key')
    }
    if (network) {
      this.network = network
    } else {
      this.apple = new AppleNetwork()
      this.network = this.apple.send
    }
  }
  close(): void {
    this.apple?.close()
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
