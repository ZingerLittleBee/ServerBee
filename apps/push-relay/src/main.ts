import { X509Certificate } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { serve } from 'bun'
import { requireValue } from './attestation'
import { Relay } from './relay'

function required(name: string): string {
  const value = process.env[name]
  requireValue(value, `Missing ${name}`)
  return value
}

const rootPem = readFileSync(required('APP_ATTEST_ROOT_CA'), 'utf8')
const root = new X509Certificate(rootPem)
requireValue(root.fingerprint256 === required('APP_ATTEST_ROOT_SHA256'), 'App Attest trust anchor pin mismatch')
const environment = required('APNS_ENVIRONMENT')
requireValue(environment === 'sandbox' || environment === 'production', 'Invalid APNS_ENVIRONMENT')
const relay = new Relay(required('RELAY_DATABASE'), {
  appId: required('APP_ATTEST_APP_ID'),
  rootPem,
  environment
})

serve({
  hostname: '127.0.0.1',
  port: Number(process.env.RELAY_PORT ?? '8787'),
  fetch(request, server) {
    // Reverse proxies must enforce their own source-IP limits. Never trust a
    // caller-supplied forwarding header as the local admission rate-limit key.
    return relay.handle(request, server.requestIP(request)?.address ?? 'unknown')
  }
})
