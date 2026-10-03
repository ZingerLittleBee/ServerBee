/** Isolated stitched-path fixture. Real admission + delivery, replacing only
 * Apple-generated attestation certificates and the outbound APNs network. */

import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { serve, sleep } from 'bun'
import { ApnsTransport } from '../src/apns'
import { Relay } from '../src/relay'
import { AppleFixture } from './fixtures'

const directory = process.argv[2]
if (!directory) {
  throw new Error('Missing isolated fixture directory')
}
const fixture = new AppleFixture()
let providerStatus = 200
let providerReason: string | undefined
const apns = new ApnsTransport(
  { teamId: 'TESTTEAM01', keyId: 'TESTKEY01', privateKey: fixture.privateKey, topic: 'com.serverbee.mobile' },
  async (request) => {
    // Read test controls at the unavoidable provider boundary, never Relay policy.
    try {
      const control = JSON.parse(readFileSync(`${directory}/provider.json`, 'utf8')) as {
        status: number
        reason?: string
        delay_ms?: number
        wait_for_release?: boolean
      }
      providerStatus = control.status
      providerReason = control.reason
      writeFileSync(`${directory}/provider-started.json`, JSON.stringify({ token: request.token }))
      if (control.wait_for_release) {
        const deadline = Date.now() + 10_000
        while (!existsSync(`${directory}/release`) && Date.now() < deadline) {
          await sleep(20)
        }
      }
      if (control.delay_ms) {
        await sleep(control.delay_ms)
      }
    } catch {
      providerStatus = 200
      providerReason = undefined
    }
    writeFileSync(`${directory}/provider-request.json`, JSON.stringify(request))
    return { status: providerStatus, reason: providerReason }
  }
)
const relay = new Relay(
  `${directory}/relay.db`,
  { ...fixture.trust(), environments: ['sandbox', 'production'] },
  undefined,
  apns
)
let counter = 0
const server = serve({
  hostname: '127.0.0.1',
  port: 0,
  async fetch(request) {
    // Synthetic Apple/native proof boundary for token-rotation race tests. The
    // resulting challenge/assertion still passes the actual Relay admission.
    if (new URL(request.url).pathname === '/fixture/renew') {
      const pending = await relay.handle(
        new Request('https://relay.test/v1/challenges', {
          method: 'POST',
          body: JSON.stringify({
            action: 'renew',
            key_id: fixture.keyId,
            device_token: 'b'.repeat(64),
            environment: 'sandbox'
          })
        }),
        'native-fixture'
      )
      const challenge = (await pending.json()) as { challenge_id: string; client_data: string }
      return relay.handle(
        new Request('https://relay.test/v1/renew', {
          method: 'POST',
          body: JSON.stringify({
            challenge_id: challenge.challenge_id,
            proof: fixture.assertion(Buffer.from(challenge.client_data, 'base64'), ++counter)
          })
        }),
        'native-fixture'
      )
    }
    return relay.handle(request, 'fixture')
  }
})
const base = `http://127.0.0.1:${server.port}`
const challengeResponse = await fetch(`${base}/v1/challenges`, {
  method: 'POST',
  body: JSON.stringify({
    action: 'attest',
    key_id: fixture.keyId,
    device_token: 'a'.repeat(64),
    environment: 'sandbox'
  })
})
if (challengeResponse.status !== 200) {
  throw new Error('Challenge rejected')
}
const challenge = (await challengeResponse.json()) as { challenge_id: string; client_data: string }
const admissionResponse = await fetch(`${base}/v1/attest`, {
  method: 'POST',
  body: JSON.stringify({
    challenge_id: challenge.challenge_id,
    proof: fixture.attestation(Buffer.from(challenge.client_data, 'base64'))
  })
})
if (admissionResponse.status !== 200) {
  throw new Error('Attestation rejected')
}
const grant: unknown = await admissionResponse.json()
writeFileSync(`${directory}/ready.json`, JSON.stringify({ url: base, grant }))
process.on('SIGTERM', () => {
  server.stop(true)
  relay.db.close()
  fixture.close()
  process.exit(0)
})
