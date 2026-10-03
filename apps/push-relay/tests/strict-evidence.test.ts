import { afterEach, expect, test } from 'bun:test'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { assertion, attest, requireValue } from '../src/attestation'
import { Relay } from '../src/relay'
import { AppleFixture } from './fixtures'

const cleanups: (() => void)[] = []
afterEach(() => {
  for (const cleanup of cleanups.splice(0).reverse()) {
    cleanup()
  }
})

function claims(attestation: boolean, version = '1.0', category = 3) {
  const encodedCategory = Buffer.alloc(4)
  encodedCategory.writeUInt32LE(category)
  return new Map<string, unknown>([
    [attestation ? 'apple_validation_category_01' : 'validationCategory', encodedCategory],
    [attestation ? 'apple_bundle_version_01' : 'bundleVersion', version]
  ])
}

function setup(requireExtensions?: boolean) {
  const fixture = new AppleFixture()
  const directory = mkdtempSync(join(tmpdir(), 'serverbee-evidence-state-'))
  const path = join(directory, 'state.db')
  let trust = { ...fixture.trust(), requireExtensions }
  let relay = new Relay(path, trust)
  cleanups.push(() => {
    relay.db.close()
    fixture.close()
    rmSync(directory, { recursive: true, force: true })
  })
  const request = (route: string, body: unknown, token?: string) =>
    relay.handle(
      new Request(`https://relay.test${route}`, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: body === undefined ? undefined : JSON.stringify(body)
      }),
      'evidence-fixture'
    )
  const challenge = async (action = 'attest', grantId?: string, deviceToken = 'a'.repeat(64)) => {
    const response = await request('/v1/challenges', {
      action,
      key_id: fixture.keyId,
      device_token: deviceToken,
      environment: 'sandbox',
      grant_id: grantId,
      requireExtensions: false
    })
    requireValue(response.status === 200, 'Fixture challenge failed')
    return (await response.json()) as { challenge_id: string; client_data: string }
  }
  const admit = async (extensions = true) => {
    const pending = await challenge()
    const response = await request('/v1/attest', {
      challenge_id: pending.challenge_id,
      proof: fixture.attestation(Buffer.from(pending.client_data, 'base64'), {
        extensions: extensions ? claims(true) : undefined
      })
    })
    requireValue(response.status === 200, 'Fixture admission failed')
    return (await response.json()) as { grant_token: string; grant_id: string }
  }
  return {
    fixture,
    challenge,
    request,
    admit,
    get relay() {
      return relay
    },
    restart(options: { requireExtensions?: boolean; bundleVersions?: string[] } = {}) {
      relay.db.close()
      trust = { ...trust, ...options }
      relay = new Relay(path, trust)
    },
    counter() {
      return relay.db.query<{ counter: number }, [string]>('SELECT counter FROM keys WHERE key_id=?').get(fixture.keyId)
        ?.counter
    }
  }
}

for (const strict of [undefined, false, true]) {
  test(`extension-free admission follows evidence mode ${String(strict)} and consumes rejected challenges`, async () => {
    const flow = setup(strict)
    const pending = await flow.challenge()
    const clientData = Buffer.from(pending.client_data, 'base64')
    const response = await flow.request('/v1/attest', {
      challenge_id: pending.challenge_id,
      proof: flow.fixture.attestation(clientData),
      requireExtensions: false
    })
    expect(response.status).toBe(strict ? 403 : 200)
    expect(flow.counter()).toBe(strict ? undefined : 0)
    if (strict) {
      expect(await response.json()).toEqual({ error: 'Device verification rejected' })
      expect(() =>
        attest(flow.fixture.attestation(clientData), flow.fixture.keyId, clientData, {
          ...flow.fixture.trust(),
          requireExtensions: true
        })
      ).toThrow('Missing required authenticator extensions')
      const retry = await flow.request('/v1/attest', {
        challenge_id: pending.challenge_id,
        proof: flow.fixture.attestation(clientData, { extensions: claims(true) })
      })
      expect(retry.status).toBe(403)
      expect(flow.counter()).toBeUndefined()
    } else {
      const grant = (await response.json()) as { grant_token: string }
      flow.restart()
      const renewal = await flow.challenge('renew')
      expect(
        (
          await flow.request('/v1/renew', {
            challenge_id: renewal.challenge_id,
            proof: flow.fixture.assertion(Buffer.from(renewal.client_data, 'base64'), 1)
          })
        ).status
      ).toBe(200)
      expect(flow.counter()).toBe(1)
      expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(403)
    }
  })
}

test('strict renewal needs fresh claims before and after restart; denial preserves grants and counters', async () => {
  const flow = setup(true)
  const grant = await flow.admit()
  for (const restart of [false, true]) {
    if (restart) {
      flow.restart()
    }
    const pending = await flow.challenge('renew')
    const clientData = Buffer.from(pending.client_data, 'base64')
    const proof = flow.fixture.assertion(clientData, 1)
    const response = await flow.request('/v1/renew', {
      challenge_id: pending.challenge_id,
      proof,
      action: 'revoke',
      requireExtensions: false
    })
    expect(response.status).toBe(403)
    expect(await response.json()).toEqual({ error: 'Device verification rejected' })
    expect(flow.counter()).toBe(0)
    expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
    expect(
      (
        await flow.request('/v1/renew', {
          challenge_id: pending.challenge_id,
          proof: flow.fixture.assertion(clientData, 1, false, { extensions: claims(false) })
        })
      ).status
    ).toBe(403)
    expect(flow.counter()).toBe(0)
    expect(() =>
      assertion(
        proof,
        flow.relay.db.query<{ public_key: string }, []>('SELECT public_key FROM keys').get()?.public_key ?? '',
        0,
        clientData,
        {
          ...flow.fixture.trust(),
          requireExtensions: true
        }
      )
    ).toThrow('Missing required authenticator extensions')
  }
  const fresh = await flow.challenge('renew')
  expect(
    (
      await flow.request('/v1/renew', {
        challenge_id: fresh.challenge_id,
        proof: flow.fixture.assertion(Buffer.from(fresh.client_data, 'base64'), 1, false, { extensions: claims(false) })
      })
    ).status
  ).toBe(200)
  expect(flow.counter()).toBe(1)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(403)
})

test('enabling strict issuance retains existing legacy grants and permits only scoped authenticated legacy revocation', async () => {
  const flow = setup()
  const grant = await flow.admit(false)
  flow.restart({ requireExtensions: true })
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
  const pending = await flow.challenge('renew')
  const proof = flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1)
  expect((await flow.request('/v1/renew', { challenge_id: pending.challenge_id, proof })).status).toBe(403)
  expect(flow.counter()).toBe(0)
  expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
  for (const failure of ['signature', 'version', 'missing-category']) {
    const rejected = await flow.challenge('revoke', grant.grant_id)
    const extensions = failure === 'signature' ? undefined : claims(false, failure === 'version' ? 'unapproved' : '1.0')
    if (failure === 'missing-category') {
      extensions?.delete('validationCategory')
    }
    const rejectedProof = flow.fixture.assertion(
      Buffer.from(rejected.client_data, 'base64'),
      1,
      failure === 'signature',
      { extensions }
    )
    expect(
      (await flow.request('/v1/revoke', { challenge_id: rejected.challenge_id, proof: rejectedProof })).status
    ).toBe(403)
    expect(flow.counter()).toBe(0)
    expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
  }
  for (const [deviceToken, counter, expectedStatus] of [
    ['b'.repeat(64), 1, 200],
    ['a'.repeat(64), 2, 403]
  ] as const) {
    const revoke = await flow.challenge('revoke', grant.grant_id, deviceToken)
    const revokeProof = flow.fixture.assertion(Buffer.from(revoke.client_data, 'base64'), counter)
    const response = await flow.request('/v1/revoke', {
      challenge_id: revoke.challenge_id,
      proof: revokeProof,
      action: 'renew'
    })
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ revoked: true })
    expect(flow.counter()).toBe(counter)
    expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(expectedStatus)
    expect((await flow.request('/v1/revoke', { challenge_id: revoke.challenge_id, proof: revokeProof })).status).toBe(
      403
    )
  }
  expect(flow.relay.db.query<{ count: number }, []>('SELECT count(*) AS count FROM grants').get()?.count).toBe(1)
})

for (const strict of [false, true]) {
  test(`current allowlist applies to renewed signed claims after restart in evidence mode ${strict}`, async () => {
    const flow = setup(strict)
    const grant = await flow.admit()
    flow.restart({ bundleVersions: ['2.0'] })
    const pending = await flow.challenge('renew')
    const proof = flow.fixture.assertion(Buffer.from(pending.client_data, 'base64'), 1, false, {
      extensions: claims(false)
    })
    expect((await flow.request('/v1/renew', { challenge_id: pending.challenge_id, proof })).status).toBe(403)
    expect(flow.counter()).toBe(0)
    expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
    const fresh = await flow.challenge('renew')
    expect(
      (
        await flow.request('/v1/renew', {
          challenge_id: fresh.challenge_id,
          proof: flow.fixture.assertion(Buffer.from(fresh.client_data, 'base64'), 1, false, {
            extensions: claims(false, '2.0')
          })
        })
      ).status
    ).toBe(200)
    expect(flow.counter()).toBe(1)
  })

  for (const failure of ['distribution', 'version', 'missing-category', 'missing-version']) {
    test(`present attestation and assertion claims reject ${failure} in evidence mode ${strict}`, async () => {
      const flow = setup(strict)
      let grant: { grant_token: string } | undefined
      for (const action of ['attest', 'renew'] as const) {
        if (action === 'renew') {
          grant = await flow.admit()
        }
        const pending = await flow.challenge(action)
        const clientData = Buffer.from(pending.client_data, 'base64')
        const isAttestation = action === 'attest'
        const extensions = claims(
          isAttestation,
          failure === 'version' ? 'unapproved' : '1.0',
          failure === 'distribution' ? 4 : 3
        )
        if (failure === 'missing-category') {
          extensions.delete(isAttestation ? 'apple_validation_category_01' : 'validationCategory')
        }
        if (failure === 'missing-version') {
          extensions.delete(isAttestation ? 'apple_bundle_version_01' : 'bundleVersion')
        }
        const proof = isAttestation
          ? flow.fixture.attestation(clientData, { extensions })
          : flow.fixture.assertion(clientData, 1, false, { extensions })
        expect((await flow.request(`/v1/${action}`, { challenge_id: pending.challenge_id, proof })).status).toBe(403)
        expect(flow.counter()).toBe(isAttestation ? undefined : 0)
        if (grant) {
          expect((await flow.request('/v1/grants/inspect', undefined, grant.grant_token)).status).toBe(200)
        }
      }
    })
  }
}
