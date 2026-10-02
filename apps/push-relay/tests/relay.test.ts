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

function appleExtensions(category = 3, version = '1.0', attestation = false) {
  const encodedCategory = Buffer.alloc(4)
  encodedCategory.writeUInt32LE(category)
  return new Map<string, unknown>([
    [attestation ? 'apple_validation_category_01' : 'validationCategory', encodedCategory],
    [attestation ? 'apple_bundle_version_01' : 'bundleVersion', version]
  ])
}

test('signed Apple extensions admit attestation, renewal and revocation; old assertions remain compatible', async () => {
  const flow = setup()
  const admission = await flow.challenge()
  const response = await flow.request('/v1/attest', {
    challenge_id: admission.challenge_id,
    proof: flow.fixture.attestation(Buffer.from(admission.client_data, 'base64'), {
      extensions: appleExtensions(3, '1.0', true)
    })
  })
  expect(response.status).toBe(200)
  const renewedChallenge = await flow.challenge('renew')
  const renewed = await flow.request('/v1/renew', {
    challenge_id: renewedChallenge.challenge_id,
    proof: flow.fixture.assertion(Buffer.from(renewedChallenge.client_data, 'base64'), 1, false, {
      extensions: appleExtensions()
    })
  })
  expect(renewed.status).toBe(200)
  const grant = (await renewed.json()) as { grant_id: string; grant_token: string }
  const revokedChallenge = await flow.challenge('revoke', 'a'.repeat(64), grant.grant_id)
  expect(
    (
      await flow.request('/v1/revoke', {
        challenge_id: revokedChallenge.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(revokedChallenge.client_data, 'base64'), 2, false, {
          extensions: appleExtensions()
        })
      })
    ).status
  ).toBe(200)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(403)
  const old = await flow.challenge('renew')
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: old.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(old.client_data, 'base64'), 3)
      })
    ).status
  ).toBe(200)
})

for (const failure of ['distribution', 'version', 'type', 'missing', 'tamper', 'flag', 'trailing', 'multiple']) {
  test(`rejects signed assertion extension ${failure} without rotating the active grant`, async () => {
    const flow = setup()
    const grant = await flow.admit()
    const pending = await flow.challenge('renew')
    const extensions = appleExtensions(failure === 'distribution' ? 4 : 3, failure === 'version' ? 'unapproved' : '1.0')
    if (failure === 'type') {
      extensions.set('validationCategory', Buffer.alloc(3))
    }
    if (failure === 'missing') {
      extensions.delete('bundleVersion')
    }
    const proof = flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1, false, {
      extensions,
      tamperExtensions: failure === 'tamper',
      omitExtensionFlag: failure === 'flag',
      trailing: ({ trailing: Buffer.from([0xff]), multiple: Buffer.from([0xa0]) } as Record<string, Buffer>)[failure]
    })
    expect((await flow.request('/v1/renew', { challenge_id: pending.challenge_id, proof })).status).toBe(403)
    expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
  })
}

for (const category of [2, 4]) {
  test(`production extensions admit distribution category ${category} with the full signed data`, async () => {
    const flow = setup()
    const production = new Relay(':memory:', flow.fixture.trust('production'))
    cleanups.push(() => production.db.close())
    const request = (path: string, body: unknown) =>
      production.handle(
        new Request(`https://relay.test${path}`, { method: 'POST', body: JSON.stringify(body) }),
        'production-extensions'
      )
    const challenge = async (action: string) => {
      const response = await request('/v1/challenges', {
        action,
        key_id: flow.fixture.keyId,
        device_token: 'a'.repeat(64),
        environment: 'production'
      })
      return (await response.json()) as { challenge_id: string; client_data: string }
    }
    const first = await challenge('attest')
    expect(
      (
        await request('/v1/attest', {
          challenge_id: first.challenge_id,
          proof: flow.fixture.attestation(Buffer.from(first.client_data, 'base64'), {
            environment: 'production',
            extensions: appleExtensions(category, '1.0', true)
          })
        })
      ).status
    ).toBe(200)
    const renewal = await challenge('renew')
    expect(
      (
        await request('/v1/renew', {
          challenge_id: renewal.challenge_id,
          proof: flow.fixture.assertion(Buffer.from(renewal.client_data, 'base64'), 1, false, {
            extensions: appleExtensions(category)
          })
        })
      ).status
    ).toBe(200)
  })
}

test('one Relay URL and database admit both environments with independent grants and counters', async () => {
  const sandbox = new AppleFixture()
  const production = new AppleFixture(sandbox)
  const directory = mkdtempSync(join(tmpdir(), 'serverbee-shared-environments-'))
  const database = join(directory, 'state.db')
  const trust = { ...sandbox.trust(), environments: ['sandbox', 'production'] as const }
  let relay = new Relay(database, trust)
  cleanups.push(() => {
    relay.db.close()
    production.close()
    sandbox.close()
    rmSync(directory, { recursive: true, force: true })
  })
  const request = (path: string, body?: unknown, token?: string) =>
    relay.handle(
      new Request(`https://shared-relay.test${path}`, {
        method: 'POST',
        body: body === undefined ? undefined : JSON.stringify(body),
        headers: token ? { Authorization: `Bearer ${token}` } : {}
      }),
      'shared-environment-test'
    )
  const challenge = async (
    fixture: AppleFixture,
    environment: string,
    action: string,
    token: string,
    grant?: string
  ) => {
    const response = await request('/v1/challenges', {
      action,
      key_id: fixture.keyId,
      environment,
      device_token: token,
      grant_id: grant
    })
    expect(response.status).toBe(200)
    return (await response.json()) as { challenge_id: string; client_data: string }
  }
  const admit = async (fixture: AppleFixture, environment: 'sandbox' | 'production', token: string) => {
    const pending = await challenge(fixture, environment, 'attest', token)
    const response = await request('/v1/attest', {
      challenge_id: pending.challenge_id,
      proof: fixture.attestation(Buffer.from(pending.client_data, 'base64'), {
        environment,
        extensions: appleExtensions(environment === 'sandbox' ? 3 : 4, '1.0', true)
      })
    })
    expect(response.status).toBe(200)
    return (await response.json()) as { grant_id: string; grant_token: string }
  }
  const dev = await admit(sandbox, 'sandbox', 'a'.repeat(64))
  const prod = await admit(production, 'production', 'b'.repeat(64))
  relay.db.close()
  relay = new Relay(database, trust)
  expect((await (await request('/v1/grants/inspect', undefined, dev.grant_token)).json()).environment).toBe('sandbox')
  expect((await (await request('/v1/grants/inspect', undefined, prod.grant_token)).json()).environment).toBe(
    'production'
  )
  const mismatch = await challenge(sandbox, 'production', 'renew', 'a'.repeat(64))
  expect(
    (
      await request('/v1/renew', {
        challenge_id: mismatch.challenge_id,
        proof: sandbox.assertion(Buffer.from(mismatch.client_data, 'base64'), 1, false, {
          extensions: appleExtensions(4)
        })
      })
    ).status
  ).toBe(403)
  const renewal = await challenge(sandbox, 'sandbox', 'renew', 'a'.repeat(64))
  const response = await request('/v1/renew', {
    challenge_id: renewal.challenge_id,
    // The later request cannot override the persisted challenge environment.
    environment: 'production',
    proof: sandbox.assertion(Buffer.from(renewal.client_data, 'base64'), 1, false, { extensions: appleExtensions() })
  })
  expect(response.status).toBe(200)
  const renewed = (await response.json()) as { grant_id: string; grant_token: string }
  expect((await request('/v1/grants/inspect', undefined, dev.grant_token)).status).toBe(403)
  expect((await request('/v1/grants/inspect', undefined, prod.grant_token)).status).toBe(200)
  const forged = await challenge(sandbox, 'sandbox', 'revoke', 'a'.repeat(64), prod.grant_id)
  expect(
    (
      await request('/v1/revoke', {
        challenge_id: forged.challenge_id,
        proof: sandbox.assertion(Buffer.from(forged.client_data, 'base64'), 2)
      })
    ).status
  ).toBe(200)
  expect((await request('/v1/grants/inspect', undefined, prod.grant_token)).status).toBe(200)
  const revoke = await challenge(production, 'production', 'revoke', 'b'.repeat(64), prod.grant_id)
  expect(
    (
      await request('/v1/revoke', {
        challenge_id: revoke.challenge_id,
        proof: production.assertion(Buffer.from(revoke.client_data, 'base64'), 1, false, {
          extensions: appleExtensions(4)
        })
      })
    ).status
  ).toBe(200)
  expect((await request('/v1/grants/inspect', undefined, prod.grant_token)).status).toBe(403)
  expect((await request('/v1/grants/inspect', undefined, renewed.grant_token)).status).toBe(200)
})
