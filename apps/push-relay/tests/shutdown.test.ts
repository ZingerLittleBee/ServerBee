import { test } from 'bun:test'
import assert from 'node:assert/strict'
import { X509Certificate } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { serve, sleep, spawn } from 'bun'
import { AppleFixture } from './fixtures'

for (const mode of ['delayed', 'stalled'] as const) {
  test(`Relay shutdown ${mode === 'delayed' ? 'waits for actual fetch completion before closing SQLite' : 'bounds an uncooperative handler without closing SQLite beneath it'}`, async () => {
    const fixture = new AppleFixture()
    const reservation = serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response('reserved') })
    const port = reservation.port
    await reservation.stop(true)
    const eventsPath = join(fixture.directory, 'events.txt')
    const events = () => (existsSync(eventsPath) ? readFileSync(eventsPath, 'utf8').trim().split('\n') : [])
    const child = spawn([process.execPath, join(import.meta.dir, 'serve-shutdown-fixture.ts')], {
      env: {
        ...process.env,
        SHUTDOWN_MODE: mode,
        SHUTDOWN_EVENTS: eventsPath,
        RELAY_TRUSTED_PROXY_IPS: '127.0.0.1',
        RELAY_DATABASE: join(fixture.directory, 'relay.sqlite'),
        RELAY_PORT: String(port),
        APP_ATTEST_ROOT_CA: join(fixture.directory, 'root.pem'),
        APP_ATTEST_ROOT_SHA256: new X509Certificate(fixture.rootPem).fingerprint256,
        APP_ATTEST_APP_ID: 'TESTTEAM01.com.serverbee.mobile',
        APP_ATTEST_BUNDLE_VERSIONS: '1.0',
        APNS_ENVIRONMENTS: 'sandbox,production',
        APNS_TEAM_ID: 'TESTTEAM01',
        APNS_KEY_ID: 'TESTKEY01',
        APNS_PRIVATE_KEY: join(fixture.directory, 'leaf.key'),
        APNS_TOPIC: 'com.serverbee.mobile'
      },
      stdout: 'pipe',
      stderr: 'pipe'
    })
    let watchdog: ReturnType<typeof setTimeout> | undefined
    try {
      async function waitFor(event: string): Promise<void> {
        const deadline = Date.now() + 3000
        while (!events().includes(event)) {
          assert(Date.now() < deadline, `Did not observe ${event}`)
          await sleep(10)
        }
      }
      await waitFor('ready')
      const response = fetch(`http://127.0.0.1:${port}/`, {
        headers: { 'x-serverbee-client-ip': '192.0.2.1' }
      }).catch(() => undefined)
      await waitFor('handler-start')
      child.kill('SIGTERM')
      const exit = await Promise.race([
        child.exited,
        new Promise<never>((_, reject) => {
          watchdog = setTimeout(() => reject(new Error('Relay shutdown was not bounded')), 7000)
        })
      ])
      await response
      const stderr = await new Response(child.stderr).text()
      if (mode === 'delayed') {
        assert.equal(exit, 0)
        assert.deepEqual(events(), ['ready', 'handler-start', 'apns-close', 'handler-finish', 'database-close'])
        assert.equal(stderr, '')
      } else {
        assert.equal(exit, 1)
        assert.deepEqual(events(), ['ready', 'handler-start', 'apns-close'])
        assert(stderr.includes('Relay shutdown timed out waiting for request handlers'))
      }
    } finally {
      clearTimeout(watchdog)
      if (child.exitCode === null) {
        child.kill('SIGKILL')
      }
      await child.exited
      fixture.close()
    }
  }, 12_000)
}
