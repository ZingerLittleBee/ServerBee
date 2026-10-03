import { X509Certificate } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { serve } from 'bun'
import { ApnsTransport } from './apns'
import { type Environment, requireValue } from './attestation'
import { Relay } from './relay'

function required(name: string): string {
  const value = process.env[name]
  requireValue(value, `Missing ${name}`)
  return value
}

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
const relay = new Relay(
  required('RELAY_DATABASE'),
  {
    appId: required('APP_ATTEST_APP_ID'),
    rootPem,
    environment,
    environments,
    bundleVersions
  },
  undefined,
  new ApnsTransport({
    teamId: required('APNS_TEAM_ID'),
    keyId: required('APNS_KEY_ID'),
    privateKey: readFileSync(required('APNS_PRIVATE_KEY'), 'utf8'),
    topic: required('APNS_TOPIC')
  })
)

serve({
  hostname: '127.0.0.1',
  port: Number(process.env.RELAY_PORT ?? '8787'),
  fetch(request, server) {
    // Reverse proxies must enforce their own source-IP limits. Never trust a
    // caller-supplied forwarding header as the local admission rate-limit key.
    return relay.handle(request, server.requestIP(request)?.address ?? 'unknown')
  }
})
