import { expect, test } from 'bun:test'
import { createPublicKey, generateKeyPairSync, verify } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { type ApnsRequest, ApnsTransport, classify } from '../src/apns'
import { Relay } from '../src/relay'
import { AppleFixture } from './fixtures'

const vector = JSON.parse(
  readFileSync(new URL('../../../tests/fixtures/push-envelope-v1.json', import.meta.url), 'utf8')
) as { envelope: unknown }
const event = '11111111-1111-4111-8111-111111111111'

test('real grant admission scopes encrypted delivery; APNs headers, environments and cached ES256 JWT', async () => {
  const fixture = new AppleFixture()
  const requests: ApnsRequest[] = []
  let now = Math.floor(Date.now() / 1000)
  const key = generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
  const apns = new ApnsTransport(
    {
      teamId: 'TESTTEAM01',
      keyId: 'TESTKEY01',
      privateKey: key.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString(),
      topic: 'com.serverbee.mobile'
    },
    (request) => {
      requests.push(request)
      return Promise.resolve({ status: 200 })
    },
    () => now
  )
  const relay = new Relay(':memory:', { ...fixture.trust(), environments: ['sandbox', 'production'] }, () => now, apns)
  const request = (path: string, body: unknown, token?: string) =>
    relay.handle(
      new Request(`https://relay.test${path}`, {
        method: 'POST',
        headers: token ? { authorization: `Bearer ${token}` } : {},
        body: JSON.stringify(body)
      }),
      'test'
    )
  try {
    for (const environment of ['sandbox', 'production'] as const) {
      const device = new AppleFixture(fixture)
      try {
        const challenge = (await (
          await request('/v1/challenges', {
            action: 'attest',
            key_id: device.keyId,
            device_token: environment === 'sandbox' ? 'a'.repeat(64) : 'b'.repeat(64),
            environment
          })
        ).json()) as { challenge_id: string; client_data: string }
        const admitted = await request('/v1/attest', {
          challenge_id: challenge.challenge_id,
          proof: device.attestation(Buffer.from(challenge.client_data, 'base64'), { environment })
        })
        expect(admitted.status).toBe(200)
        const grant = (await admitted.json()) as { grant_token: string; grant_id: string }
        const body = { event_id: event, expires_at: now + 1800, envelope: vector.envelope }
        expect((await request('/v1/send', body)).status).toBe(403)
        expect((await request('/v1/send', { ...body, device_token: 'c'.repeat(64) }, grant.grant_token)).status).toBe(
          403
        )
        expect(
          (
            await request(
              '/v1/send',
              { ...body, envelope: { ...(vector.envelope as object), content_key: 'secret' } },
              grant.grant_token
            )
          ).status
        ).toBe(403)
        expect((await (await request('/v1/send', body, grant.grant_token)).json()).outcome).toBe('accepted')
        const sent = requests.at(-1)
        expect(sent?.environment).toBe(environment)
        expect(sent?.token).toBe(environment === 'sandbox' ? 'a'.repeat(64) : 'b'.repeat(64))
        expect(sent?.headers['apns-topic']).toBe('com.serverbee.mobile')
        expect(sent?.headers['apns-push-type']).toBe('alert')
        expect(sent?.headers['apns-priority']).toBe('10')
        expect(sent?.headers['apns-expiration']).toBe(String(now + 1800))
        expect(sent?.headers['apns-id']).toBe(event)
        expect(JSON.parse(sent?.payload ?? '{}').aps['mutable-content']).toBe(1)
        expect(sent?.payload).not.toContain('deployment_id')
        expect(sent?.payload).not.toContain('content_key')
        expect(Buffer.byteLength(sent?.payload ?? '')).toBeLessThanOrEqual(4096)
        relay.db.run('UPDATE grants SET revoked=1 WHERE grant_id=?', [grant.grant_id])
        expect((await request('/v1/send', body, grant.grant_token)).status).toBe(403)
      } finally {
        device.close()
      }
    }
    expect(requests.length).toBe(2)
    expect(requests[0]?.headers.authorization).toBe(requests[1]?.headers.authorization)
    const jwt = requests[0]?.headers.authorization.slice(7).split('.') ?? []
    expect(
      verify(
        'sha256',
        Buffer.from(`${jwt[0]}.${jwt[1]}`),
        { key: createPublicKey(key.privateKey), dsaEncoding: 'ieee-p1363' },
        Buffer.from(jwt[2] ?? '', 'base64url')
      )
    ).toBe(true)
    now += 3001
    await apns.send('a'.repeat(64), 'sandbox', event, now + 1800, vector.envelope)
    expect(requests[2]?.headers.authorization).not.toBe(requests[0]?.headers.authorization)
    const before = requests.length
    expect((await apns.send('a'.repeat(64), 'sandbox', event, now, vector.envelope)).outcome).toBe('expired')
    expect(
      (await apns.send('a'.repeat(64), 'sandbox', event, now + 1800, { ciphertext: 'a'.repeat(5000) })).reason
    ).toBe('PayloadTooLarge')
    expect(requests.length).toBe(before)
  } finally {
    relay.db.close()
    fixture.close()
  }
})

test('provider failures distinguish terminal device state, configuration, payload and transient errors', () => {
  expect(classify({ status: 410, reason: 'Unregistered' }).device_invalid).toBe(true)
  for (const reason of ['BadDeviceToken', 'DeviceTokenNotForTopic', 'BadTopic', 'BadPayload', 'ExpiredProviderToken']) {
    expect(classify({ status: 400, reason }).device_invalid).toBe(false)
    expect(classify({ status: 400, reason }).outcome).toBe('permanent')
  }
  for (const status of [429, 500, 503]) {
    expect(classify({ status }).outcome).toBe('retryable')
  }
})
