import { Database } from 'bun:sqlite'
import { randomBytes, randomUUID } from 'node:crypto'
import { assertion, attest, type Environment, hash, requireValue, type Trust } from './attestation'

const tokenPattern = /^[a-f0-9]{64}$/
const keyPattern = /^[A-Za-z0-9+/]{43}=$/

class BodyTooLarge extends Error {}

async function readBody(request: Request): Promise<Record<string, unknown>> {
  const reader = request.body?.getReader()
  requireValue(reader, 'Missing body')
  const chunks: Uint8Array[] = []
  let size = 0
  for (;;) {
    const { value, done } = await reader.read()
    if (done) {
      break
    }
    size += value.byteLength
    if (size > 32_768) {
      await reader.cancel()
      throw new BodyTooLarge()
    }
    chunks.push(value)
  }
  const body: unknown = JSON.parse(Buffer.concat(chunks).toString())
  requireValue(body && typeof body === 'object' && !Array.isArray(body), 'Invalid request body')
  return body as Record<string, unknown>
}

interface Challenge {
  action: string
  client_data: string
  device_token: string
  environment: Environment
  expires_at: number
  grant_id: string | null
  id: string
  key_id: string
}
interface Key {
  counter: number
  environment: Environment
  key_id: string
  public_key: string
}
interface Grant {
  device_token: string
  environment: Environment
  expires_at: number
  grant_id: string
  key_id: string
  revoked: number
  token_hash: string
}

/** No public-send endpoint or alternate unverified admission exists. Delivery
 * is a later ticket; inspection authorizes only the exact device grant. */
export class Relay {
  readonly db: Database
  private readonly trust: Trust
  private readonly environments: readonly Environment[]
  private readonly clock: () => number

  constructor(
    path: string,
    trust: Trust & { environments?: readonly Environment[] },
    clock: () => number = () => Math.floor(Date.now() / 1000)
  ) {
    this.trust = trust
    this.environments = trust.environments ?? [trust.environment]
    requireValue(
      this.environments.length > 0 && this.environments.every((value) => value === 'sandbox' || value === 'production'),
      'Invalid admission environments'
    )
    this.clock = clock
    this.db = new Database(path, { create: true, strict: true })
    this.db.exec(`PRAGMA journal_mode=WAL;
      CREATE TABLE IF NOT EXISTS challenges (
        id TEXT PRIMARY KEY, client_data TEXT NOT NULL, action TEXT NOT NULL,
        key_id TEXT NOT NULL, device_token TEXT NOT NULL, environment TEXT NOT NULL,
        expires_at INTEGER NOT NULL, grant_id TEXT
      );
      CREATE TABLE IF NOT EXISTS keys (
        key_id TEXT PRIMARY KEY, public_key TEXT NOT NULL, environment TEXT NOT NULL,
        counter INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE IF NOT EXISTS grants (
        grant_id TEXT PRIMARY KEY, key_id TEXT NOT NULL, device_token TEXT NOT NULL,
        environment TEXT NOT NULL, token_hash TEXT UNIQUE NOT NULL, expires_at INTEGER NOT NULL,
        revoked INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE IF NOT EXISTS request_limits (source TEXT PRIMARY KEY, window INTEGER NOT NULL, count INTEGER NOT NULL);`)
  }

  private limited(source: string, now: number) {
    return this.db.transaction(() => {
      this.db.run('DELETE FROM request_limits WHERE window < ?', [now - 60])
      this.db.run(
        `INSERT INTO request_limits VALUES (?, ?, 1)
        ON CONFLICT(source) DO UPDATE SET count=count+1`,
        [source, now]
      )
      const row = this.db
        .query<{ count: number }, [string]>('SELECT count FROM request_limits WHERE source=?')
        .get(source)
      return !row || row.count > 30
    })()
  }

  private inspect(request: Request, now: number): Response {
    const authorization = request.headers.get('authorization')
    const secret = authorization?.startsWith('Bearer ') ? authorization.slice(7) : null
    requireValue(secret && secret.length < 256, 'Missing grant')
    const grant = this.db
      .query<Grant, [string]>('SELECT * FROM grants WHERE token_hash=? AND revoked=0')
      .get(hash(secret).toString('hex'))
    requireValue(
      grant && grant.expires_at > now && this.environments.includes(grant.environment),
      'Expired, revoked or disabled grant'
    )
    return Response.json(
      {
        grant_id: grant.grant_id,
        key_id: grant.key_id,
        device_token: grant.device_token,
        environment: grant.environment,
        expires_at: grant.expires_at
      },
      { headers: { 'Cache-Control': 'no-store' } }
    )
  }

  private createChallenge(body: Record<string, unknown>, now: number): Response {
    requireValue(['attest', 'renew', 'revoke'].includes(String(body.action)), 'Invalid action')
    const environment = body.environment
    requireValue(environment === 'sandbox' || environment === 'production', 'Invalid environment')
    requireValue(this.environments.includes(environment), 'Environment mismatch')
    requireValue(typeof body.device_token === 'string' && tokenPattern.test(body.device_token), 'Invalid token')
    requireValue(typeof body.key_id === 'string' && keyPattern.test(body.key_id), 'Invalid key ID')
    requireValue(
      body.action !== 'revoke' || (typeof body.grant_id === 'string' && body.grant_id.length <= 64),
      'Missing grant scope'
    )
    const keyId = body.key_id
    const deviceToken = body.device_token
    const grantScope = body.action === 'revoke' ? String(body.grant_id) : null
    const id = randomUUID()
    const clientData = JSON.stringify({
      nonce: randomBytes(32).toString('base64'),
      challenge_id: id,
      action: body.action,
      key_id: keyId,
      device_token: deviceToken,
      environment: body.environment,
      grant_id: grantScope
    })
    this.db.transaction(() => {
      this.db.run('DELETE FROM challenges WHERE expires_at <= ?', [now])
      const pending = this.db.query<{ count: number }, []>('SELECT COUNT(*) AS count FROM challenges').get()
      requireValue(pending && pending.count < 10_000, 'Too many pending challenges')
      this.db.run('INSERT INTO challenges VALUES (?, ?, ?, ?, ?, ?, ?, ?)', [
        id,
        clientData,
        String(body.action),
        keyId,
        deviceToken,
        environment,
        now + 300,
        grantScope
      ])
    })()
    return Response.json(
      { challenge_id: id, client_data: Buffer.from(clientData).toString('base64') },
      { headers: { 'Cache-Control': 'no-store' } }
    )
  }

  async handle(request: Request, source: string): Promise<Response> {
    const now = this.clock()
    if (this.limited(source, now)) {
      return Response.json({ error: 'Rate limited' }, { status: 429 })
    }
    try {
      const path = new URL(request.url).pathname
      if (request.method !== 'POST') {
        return new Response(null, { status: 404 })
      }
      if (path === '/v1/grants/inspect') {
        return this.inspect(request, now)
      }
      if (!['/v1/challenges', '/v1/attest', '/v1/renew', '/v1/revoke'].includes(path)) {
        return new Response(null, { status: 404 })
      }
      const body = await readBody(request)
      if (path === '/v1/challenges') {
        return this.createChallenge(body, now)
      }
      requireValue(typeof body.challenge_id === 'string' && typeof body.proof === 'string', 'Missing proof')
      // Consume even invalid attempts; synchronous transactions serialize replay.
      const challenge = this.db
        .query<Challenge, [string]>('DELETE FROM challenges WHERE id=? RETURNING *')
        .get(body.challenge_id)
      requireValue(challenge && challenge.expires_at > now, 'Expired or consumed challenge')
      requireValue(path === `/v1/${challenge.action}`, 'Challenge action mismatch')
      const clientData = Buffer.from(challenge.client_data)
      // Select cryptographic expectations from the persisted one-time challenge,
      // never from a proof body or from a caller's subsequent environment claim.
      requireValue(this.environments.includes(challenge.environment), 'Environment disabled')
      const trust = { ...this.trust, environment: challenge.environment }
      let publicKey: string | undefined
      let nextCounter: number | undefined
      if (challenge.action === 'attest') {
        publicKey = attest(body.proof, challenge.key_id, clientData, trust)
      } else {
        const key = this.db.query<Key, [string]>('SELECT * FROM keys WHERE key_id=?').get(challenge.key_id)
        requireValue(key && key.environment === challenge.environment, 'Unknown device key')
        nextCounter = assertion(body.proof, key.public_key, key.counter, clientData, trust)
      }
      const token = randomBytes(32).toString('base64url')
      const grantId = randomUUID()
      this.db.transaction(() => {
        if (publicKey) {
          // A key cannot be re-attested or reset to counter zero.
          this.db.run('INSERT INTO keys VALUES (?, ?, ?, 0)', [challenge.key_id, publicKey, challenge.environment])
        } else {
          const result = this.db.run('UPDATE keys SET counter=? WHERE key_id=? AND counter < ?', [
            nextCounter ?? 0,
            challenge.key_id,
            nextCounter ?? 0
          ])
          requireValue(result.changes === 1, 'Replayed assertion')
        }
        if (challenge.action === 'revoke') {
          this.db.run(
            'UPDATE grants SET revoked=1 WHERE grant_id=? AND key_id=? AND device_token=? AND environment=?',
            [challenge.grant_id, challenge.key_id, challenge.device_token, challenge.environment]
          )
        } else {
          // Rotation invalidates all earlier grants of this device key.
          this.db.run('UPDATE grants SET revoked=1 WHERE key_id=?', [challenge.key_id])
          this.db.run('INSERT INTO grants VALUES (?, ?, ?, ?, ?, ?, 0)', [
            grantId,
            challenge.key_id,
            challenge.device_token,
            challenge.environment,
            hash(token).toString('hex'),
            now + 86_400
          ])
        }
      })()
      return Response.json(
        challenge.action === 'revoke'
          ? { revoked: true }
          : {
              grant_id: grantId,
              grant_token: token,
              key_id: challenge.key_id,
              device_token: challenge.device_token,
              environment: challenge.environment,
              expires_at: now + 86_400
            },
        { headers: { 'Cache-Control': 'no-store' } }
      )
    } catch (error) {
      if (error instanceof BodyTooLarge) {
        return new Response(null, { status: 413 })
      }
      // Do not log request bodies, tokens, assertions or certificate receipts.
      return Response.json({ error: 'Device verification rejected' }, { status: 403 })
    }
  }
}
