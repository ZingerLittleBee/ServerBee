import { expect, test } from 'bun:test'
import { X509Certificate } from 'node:crypto'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { serve, sleep, spawn } from 'bun'
import { AppleFixture } from './fixtures'

async function start(raw: string | undefined) {
  const fixture = new AppleFixture()
  const reservation = serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response('reserved') })
  const port = reservation.port
  await reservation.stop(true)
  const database = join(fixture.directory, 'relay.sqlite')
  const env: Record<string, string | undefined> = {
    ...process.env,
    RELAY_TRUSTED_PROXY_IPS: '127.0.0.1',
    RELAY_DATABASE: database,
    RELAY_PORT: String(port),
    APP_ATTEST_ROOT_CA: join(fixture.directory, 'root.pem'),
    APP_ATTEST_ROOT_SHA256: new X509Certificate(fixture.rootPem).fingerprint256,
    APP_ATTEST_APP_ID: 'TESTTEAM01.com.serverbee.mobile',
    APP_ATTEST_BUNDLE_VERSIONS: '1.0',
    APP_ATTEST_REQUIRE_EXTENSIONS: raw,
    APNS_ENVIRONMENTS: 'sandbox,production',
    APNS_TEAM_ID: 'TESTTEAM01',
    APNS_KEY_ID: 'TESTKEY01',
    APNS_PRIVATE_KEY: join(fixture.directory, 'leaf.key'),
    APNS_TOPIC: 'com.serverbee.mobile'
  }
  const child = spawn([process.execPath, join(import.meta.dir, '../src/main.ts')], {
    env,
    stdout: 'pipe',
    stderr: 'pipe'
  })
  const request = (path: string, body: unknown) =>
    fetch(`http://127.0.0.1:${port}${path}`, {
      method: 'POST',
      headers: { 'x-serverbee-client-ip': '192.0.2.1' },
      body: JSON.stringify(body)
    })
  return {
    child,
    fixture,
    database,
    request,
    async ready() {
      const deadline = Date.now() + 3000
      for (;;) {
        if (child.exitCode !== null) {
          throw new Error('Relay exited before accepting requests')
        }
        const response = await request('/startup-probe', {}).catch(() => undefined)
        if (response?.status === 404) {
          return
        }
        if (Date.now() >= deadline) {
          throw new Error('Relay did not start')
        }
        await sleep(10)
      }
    },
    async close() {
      if (child.exitCode === null) {
        child.kill('SIGTERM')
      }
      const watchdog = setTimeout(() => child.kill('SIGKILL'), 3000)
      await child.exited
      clearTimeout(watchdog)
      fixture.close()
    }
  }
}

for (const raw of [undefined, 'false', 'true']) {
  test(`real Relay startup uses APP_ATTEST_REQUIRE_EXTENSIONS=${String(raw)}`, async () => {
    const flow = await start(raw)
    try {
      await flow.ready()
      for (const extensions of [false, true]) {
        if (extensions && raw !== 'true') {
          continue
        }
        const challenge = await flow.request('/v1/challenges', {
          action: 'attest',
          key_id: flow.fixture.keyId,
          device_token: 'a'.repeat(64),
          environment: 'sandbox'
        })
        expect(challenge.status).toBe(200)
        const pending = (await challenge.json()) as { challenge_id: string; client_data: string }
        const response = await flow.request('/v1/attest', {
          challenge_id: pending.challenge_id,
          proof: flow.fixture.attestation(Buffer.from(pending.client_data, 'base64'), {
            extensions: extensions
              ? new Map<string, unknown>([
                  ['apple_validation_category_01', 3],
                  ['apple_bundle_version_01', '1.0']
                ])
              : undefined
          })
        })
        expect(response.status).toBe(raw === 'true' && !extensions ? 403 : 200)
      }
    } finally {
      await flow.close()
    }
    expect(flow.child.exitCode).toBe(0)
    expect(await new Response(flow.child.stderr).text()).toBe('')
  }, 10_000)
}

for (const raw of ['', 'TRUE', '1', '0', ' true ', 'false ']) {
  test(`real Relay startup rejects invalid evidence setting ${JSON.stringify(raw)}`, async () => {
    const flow = await start(raw)
    const watchdog = setTimeout(() => flow.child.kill('SIGKILL'), 3000)
    try {
      const exit = await flow.child.exited
      expect(exit).not.toBe(0)
      expect(await new Response(flow.child.stderr).text()).toContain(
        'Invalid APP_ATTEST_REQUIRE_EXTENSIONS: expected true or false'
      )
      expect(existsSync(flow.database)).toBe(false)
    } finally {
      clearTimeout(watchdog)
      await flow.close()
    }
  }, 5000)
}
