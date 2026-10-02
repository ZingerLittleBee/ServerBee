import { afterEach, expect, test } from 'bun:test'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { requireValue } from '../src/attestation'
import { Relay } from '../src/relay'
import { AppleFixture } from './fixtures'

const cleanups: (() => void)[] = []
afterEach(() => {
  for (const cleanup of cleanups.splice(0).reverse()) {
    cleanup()
  }
})

function setup() {
  const fixture = new AppleFixture()
  const directory = mkdtempSync(join(tmpdir(), 'serverbee-relay-state-'))
  const path = join(directory, 'state.db')
  let now = Math.floor(Date.now() / 1000)
  let relay = new Relay(path, fixture.trust(), () => now)
  cleanups.push(() => {
    relay.db.close()
    fixture.close()
    rmSync(directory, { recursive: true, force: true })
  })
  const request = (route: string, body: unknown, token?: string, source = 'fixture') =>
    relay.handle(
      new Request(`https://relay.test${route}`, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: body === undefined ? undefined : JSON.stringify(body)
      }),
      source
    )
  const challenge = async (action = 'attest', deviceToken = 'a'.repeat(64), grantId?: string) => {
    const response = await request('/v1/challenges', {
      action,
      key_id: fixture.keyId,
      device_token: deviceToken,
      environment: 'sandbox',
      grant_id: grantId
    })
    requireValue(response.status === 200, 'Fixture setup request failed')
    return (await response.json()) as { challenge_id: string; client_data: string }
  }
  const admit = async () => {
    const pending = await challenge()
    const response = await request('/v1/attest', {
      challenge_id: pending.challenge_id,
      proof: fixture.attestation(Buffer.from(pending.client_data, 'base64'))
    })
    requireValue(response.status === 200, 'Fixture setup request failed')
    return (await response.json()) as { grant_token: string; grant_id: string }
  }
  return {
    fixture,
    challenge,
    admit,
    request,
    relay,
    advance(seconds: number) {
      now += seconds
    },
    restart() {
      relay.db.close()
      relay = new Relay(path, fixture.trust(), () => now)
      return relay
    }
  }
}

test('verified admission survives restart; renewal rotates and scoped revocation removes grants', async () => {
  const flow = setup()
  const initial = await flow.admit()
  flow.restart()
  expect((await flow.request('/v1/grants/inspect', undefined, initial.grant_token)).status).toBe(200)
  const pending = await flow.challenge('renew', 'b'.repeat(64))
  const proof = flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1)
  const renewed = await flow.request('/v1/renew', { challenge_id: pending.challenge_id, proof })
  expect(renewed.status).toBe(200)
  const grant = (await renewed.json()) as { grant_token: string; grant_id: string }
  expect((await flow.request('/v1/grants/inspect', undefined, initial.grant_token)).status).toBe(403)
  const inspect = await flow.request('/v1/grants/inspect', undefined, grant.grant_token)
  expect((await inspect.json()).device_token).toBe('b'.repeat(64))
  expect((await flow.request('/v1/renew', { challenge_id: pending.challenge_id, proof })).status).toBe(403)
  const revokeOther = await flow.challenge('revoke', 'a'.repeat(64), grant.grant_id)
  expect(
    (
      await flow.request('/v1/revoke', {
        challenge_id: revokeOther.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(revokeOther.client_data, 'base64'), 2)
      })
    ).status
  ).toBe(200)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
  const revoke = await flow.challenge('revoke', 'b'.repeat(64), grant.grant_id)
  expect(
    (
      await flow.request('/v1/revoke', {
        challenge_id: revoke.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(revoke.client_data, 'base64'), 3)
      })
    ).status
  ).toBe(200)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(403)
})

test.each([
  ['nonce', { wrongNonce: true }],
  ['app', { wrongApp: true }],
  ['environment', { environment: 'production' as const }],
  ['credential', { wrongCredential: true }],
  ['COSE', { wrongCose: true }],
  ['expired certificate', { expired: true }]
])('rejects %s mismatches and consumes invalid challenges', async (_, options) => {
  const flow = setup()
  const pending = await flow.challenge()
  const proof = flow.fixture.attestation(Buffer.from(pending.client_data, 'base64'), options)
  expect((await flow.request('/v1/attest', { challenge_id: pending.challenge_id, proof })).status).toBe(403)
  const valid = flow.fixture.attestation(Buffer.from(pending.client_data, 'base64'))
  expect((await flow.request('/v1/attest', { challenge_id: pending.challenge_id, proof: valid })).status).toBe(403)
})

test('rejects an untrusted chain and wrong key identifier', async () => {
  const flow = setup()
  const other = new AppleFixture()
  cleanups.push(() => other.close())
  const pending = await flow.challenge()
  expect(
    (
      await flow.request('/v1/attest', {
        challenge_id: pending.challenge_id,
        proof: other.attestation(Buffer.from(pending.client_data, 'base64'))
      })
    ).status
  ).toBe(403)
  const validResponse = await flow.request('/v1/challenges', {
    action: 'attest',
    key_id: other.keyId,
    device_token: 'a'.repeat(64),
    environment: 'sandbox'
  })
  const valid = (await validResponse.json()) as { challenge_id: string; client_data: string }
  expect(
    (
      await flow.request('/v1/attest', {
        challenge_id: valid.challenge_id,
        proof: flow.fixture.attestation(Buffer.from(valid.client_data, 'base64'))
      })
    ).status
  ).toBe(403)
})

test('rejects assertion replay, bad signature, wrong challenge and expired grants', async () => {
  const flow = setup()
  await flow.admit()
  const pending = await flow.challenge('renew')
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: pending.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1, true)
      })
    ).status
  ).toBe(403)
  const fresh = await flow.challenge('renew')
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: fresh.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1)
      })
    ).status
  ).toBe(403)
  const accepted = await flow.challenge('renew')
  const renewed = await flow.request('/v1/renew', {
    challenge_id: accepted.challenge_id,
    proof: flow.fixture.assertion(Buffer.from(accepted.client_data, 'base64'), 1)
  })
  expect(renewed.status).toBe(200)
  const active = (await renewed.json()) as { grant_token: string }
  expect((await flow.request('/v1/grants/inspect', undefined, active.grant_token)).status).toBe(200)
  const replay = await flow.challenge('renew')
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: replay.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(replay.client_data, 'base64'), 1)
      })
    ).status
  ).toBe(403)
  const expired = await flow.challenge('renew')
  flow.advance(301)
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: expired.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(expired.client_data, 'base64'), 2)
      })
    ).status
  ).toBe(403)
  flow.advance(86_400)
  expect((await flow.request('/v1/grants/inspect', undefined, active.grant_token)).status).toBe(403)
})

test('production only admits production proof and bound actions cannot be interchanged', async () => {
  const flow = setup()
  const production = new Relay(':memory:', flow.fixture.trust('production'))
  cleanups.push(() => production.db.close())
  const request = (path: string, body: unknown) =>
    production.handle(
      new Request(`https://relay.test${path}`, { method: 'POST', body: JSON.stringify(body) }),
      'production-fixture'
    )
  expect(
    (
      await request('/v1/challenges', {
        action: 'attest',
        key_id: flow.fixture.keyId,
        device_token: 'a'.repeat(64),
        environment: 'sandbox'
      })
    ).status
  ).toBe(403)
  const response = await request('/v1/challenges', {
    action: 'attest',
    key_id: flow.fixture.keyId,
    device_token: 'a'.repeat(64),
    environment: 'production'
  })
  const pending = (await response.json()) as { challenge_id: string; client_data: string }
  expect(
    (
      await request('/v1/attest', {
        challenge_id: pending.challenge_id,
        proof: flow.fixture.attestation(Buffer.from(pending.client_data, 'base64'), { environment: 'production' })
      })
    ).status
  ).toBe(200)
  const wrongAction = await flow.challenge()
  expect((await flow.request('/v1/renew', { challenge_id: wrongAction.challenge_id, proof: 'invalid' })).status).toBe(
    403
  )
})

test('bounds bodies and request rates; no public send or verification bypass exists', async () => {
  const flow = setup()
  expect((await flow.request('/v1/send', {})).status).toBe(404)
  expect((await flow.request('/v1/challenges', { padding: 'a'.repeat(40_000) })).status).toBe(413)
  for (let index = 0; index < 30; index++) {
    await flow.request('/v1/challenges', {}, undefined, 'rate-test')
  }
  expect((await flow.request('/v1/challenges', {}, undefined, 'rate-test')).status).toBe(429)
  expect((await flow.request('/v1/attest', { skip_verification: true })).status).toBe(403)
})

test('re-attestation cannot reset counters or invalidate the admitted grant', async () => {
  const flow = setup()
  const grant = await flow.admit()
  const duplicate = await flow.challenge()
  expect(
    (
      await flow.request('/v1/attest', {
        challenge_id: duplicate.challenge_id,
        proof: flow.fixture.attestation(Buffer.from(duplicate.client_data, 'base64'))
      })
    ).status
  ).toBe(403)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
})
