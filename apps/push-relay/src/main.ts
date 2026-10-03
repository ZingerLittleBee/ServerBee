import { X509Certificate } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { serve } from 'bun'
import { ApnsTransport } from './apns'
import { type Environment, requireValue } from './attestation'
import { createRelayFetch, parseTrustedProxyIPs } from './client-address'
import { Relay } from './relay'

function required(name: string): string {
  const value = process.env[name]
  requireValue(value, `Missing ${name}`)
  return value
}

// No implicit loopback trust: operators must identify the TLS proxy peer.
const trustedProxies = parseTrustedProxyIPs(required('RELAY_TRUSTED_PROXY_IPS'))
const rootPem = readFileSync(required('APP_ATTEST_ROOT_CA'), 'utf8')
const root = new X509Certificate(rootPem)
requireValue(root.fingerprint256 === required('APP_ATTEST_ROOT_SHA256'), 'App Attest trust anchor pin mismatch')
const configuredEnvironments = process.env.APNS_ENVIRONMENTS ?? required('APNS_ENVIRONMENT')
const environments: Environment[] = configuredEnvironments.split(',').map((raw) => {
  const value = raw.trim()
  requireValue(value === 'sandbox' || value === 'production', 'Invalid APNS_ENVIRONMENTS')
  return value
})
const environment = environments[0]
requireValue(environment === 'sandbox' || environment === 'production', 'Invalid default environment')
const bundleVersions = required('APP_ATTEST_BUNDLE_VERSIONS')
  .split(',')
  .map((version) => version.trim())
requireValue(
  bundleVersions.every((version) => version.length > 0 && version.length <= 128),
  'Invalid app versions'
)
const requireExtensions = process.env.APP_ATTEST_REQUIRE_EXTENSIONS ?? 'false'
requireValue(
  requireExtensions === 'true' || requireExtensions === 'false',
  'Invalid APP_ATTEST_REQUIRE_EXTENSIONS: expected true or false'
)
const apns = new ApnsTransport({
  teamId: required('APNS_TEAM_ID'),
  keyId: required('APNS_KEY_ID'),
  privateKey: readFileSync(required('APNS_PRIVATE_KEY'), 'utf8'),
  topic: required('APNS_TOPIC')
})
const relay = new Relay(
  required('RELAY_DATABASE'),
  {
    appId: required('APP_ATTEST_APP_ID'),
    rootPem,
    environment,
    environments,
    bundleVersions,
    requireExtensions: requireExtensions === 'true'
  },
  undefined,
  apns
)

const relayFetch = createRelayFetch(relay, trustedProxies)
const active = new Set<Promise<Response>>()
let shuttingDown = false
const server = serve({
  hostname: '127.0.0.1',
  port: Number(process.env.RELAY_PORT ?? '8787'),
  fetch(request, peer) {
    if (shuttingDown) {
      return new Response('Relay is shutting down', { status: 503 })
    }
    const handler = Promise.resolve(relayFetch(request, peer))
    active.add(handler)
    return handler.finally(() => active.delete(handler))
  }
})

async function shutdown(): Promise<void> {
  if (shuttingDown) {
    return
  }
  shuttingDown = true
  // Bun stop(true) can resolve before an async fetch handler finishes. Never
  // close SQLite underneath one; an uncooperative handler forces process exit.
  const deadline = setTimeout(() => {
    console.error('Relay shutdown timed out waiting for request handlers')
    process.exit(1)
  }, 5000)
  try {
    apns.close()
    await server.stop(true)
    await Promise.allSettled(active)
    relay.db.close()
  } catch (error) {
    console.error('Relay shutdown failed', error)
    process.exit(1)
  } finally {
    clearTimeout(deadline)
  }
}
process.once('SIGINT', shutdown)
process.once('SIGTERM', shutdown)
