import { execFileSync } from 'node:child_process'
import { createHash, createPublicKey, verify, X509Certificate } from 'node:crypto'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Decoder } from 'cbor-x'

const nonceDumpPattern = /OCTET STRING\s+\[HEX DUMP\]:([0-9A-F]+)$/
const noncePattern = /^3024A1220420[0-9A-F]{64}$/
const decoder = new Decoder({ mapsAsObjects: true, useRecords: false })
export const hash = (data: Uint8Array | string) => createHash('sha256').update(data).digest()
export type Environment = 'sandbox' | 'production'
export interface Trust {
  appId: string
  bundleVersions?: readonly string[]
  environment: Environment
  requireExtensions?: boolean
  rootPem: string
}

export function requireValue(value: unknown, message: string): asserts value {
  if (!value) {
    throw new Error(message)
  }
}

export function bytes(value: unknown): Buffer {
  requireValue(value instanceof Uint8Array, 'Expected CBOR bytes')
  return Buffer.from(value)
}

function object(encoded: string): Record<string, unknown> {
  requireValue(encoded.length < 24_000, 'Attestation too large')
  const value: unknown = decoder.decode(Buffer.from(encoded, 'base64'))
  requireValue(value && typeof value === 'object' && !Array.isArray(value), 'Invalid CBOR object')
  return value as Record<string, unknown>
}

function rp(data: Buffer, trust: Trust) {
  requireValue(data.length >= 37 && data.subarray(0, 32).equals(hash(trust.appId)), 'App ID mismatch')
}

/** OpenSSL performs path, signature, CA constraints and validity checks. Only
 * the configured App Attest root is trusted; the OS TLS store is not used. */
function credential(certificates: Buffer[], rootPem: string): { cert: X509Certificate; nonce: Buffer } {
  requireValue(certificates.length === 2, 'Expected App Attest leaf and intermediate')
  const directory = mkdtempSync(join(tmpdir(), 'serverbee-attest-'))
  try {
    const leaf = new X509Certificate(certificates[0])
    const intermediate = new X509Certificate(certificates[1])
    const root = new X509Certificate(rootPem)
    requireValue(root.ca && intermediate.ca && !leaf.ca, 'Invalid CA constraints')
    writeFileSync(join(directory, 'root.pem'), root.toString())
    writeFileSync(join(directory, 'chain.pem'), intermediate.toString())
    writeFileSync(join(directory, 'leaf.pem'), leaf.toString())
    writeFileSync(join(directory, 'leaf.der'), certificates[0])
    execFileSync(
      'openssl',
      [
        'verify',
        '-no-CAfile',
        '-no-CApath',
        '-no-CAstore',
        '-trusted',
        join(directory, 'root.pem'),
        '-untrusted',
        join(directory, 'chain.pem'),
        join(directory, 'leaf.pem')
      ],
      { timeout: 5000, stdio: 'pipe' }
    )
    // OpenSSL decodes DER; accept exactly Apple's SEQUENCE/[1]/OCTET STRING.
    const output = execFileSync('openssl', ['asn1parse', '-inform', 'DER', '-in', join(directory, 'leaf.der')], {
      timeout: 5000,
      encoding: 'utf8'
    })
    const lines = output.split('\n')
    const indices = lines.flatMap((line, index) =>
      line.includes('OBJECT') && line.trimEnd().endsWith(':1.2.840.113635.100.8.2') ? [index] : []
    )
    requireValue(indices.length === 1, 'Missing or duplicate nonce extension')
    const encoded = lines[indices[0] + 1]?.match(nonceDumpPattern)?.[1]
    requireValue(encoded && noncePattern.test(encoded), 'Invalid nonce extension')
    return { cert: leaf, nonce: Buffer.from(encoded.slice(12), 'hex') }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

function validateExtensions(value: unknown, trust: Trust, assertionFormat: boolean) {
  requireValue(value instanceof Map, 'Invalid authenticator extensions')
  const categoryKey = assertionFormat ? 'validationCategory' : 'apple_validation_category_01'
  const versionKey = assertionFormat ? 'bundleVersion' : 'apple_bundle_version_01'
  const encodedCategory = value.get(categoryKey)
  // Apple's published attestation vector uses a four-byte little-endian UInt32.
  const category =
    encodedCategory instanceof Uint8Array && encodedCategory.length === 4
      ? Buffer.from(encodedCategory).readUInt32LE()
      : encodedCategory
  requireValue(
    typeof category === 'number' &&
      Number.isInteger(category) &&
      (trust.environment === 'sandbox' ? category === 3 : category === 2 || category === 4),
    'App distribution mismatch'
  )
  const version = value.get(versionKey)
  requireValue(typeof version === 'string' && trust.bundleVersions?.includes(version), 'Unapproved app version')
}

export function attest(encoded: string, keyId: string, clientData: Buffer, trust: Trust): string {
  const value = object(encoded)
  requireValue(value.fmt === 'apple-appattest', 'Wrong attestation format')
  const statement = value.attStmt as Record<string, unknown>
  requireValue(statement && Array.isArray(statement.x5c), 'Missing certificate chain')
  requireValue(bytes(statement.receipt).length > 0, 'Missing Apple receipt')
  const data = bytes(value.authData)
  rp(data, trust)
  // biome-ignore lint/suspicious/noBitwiseOperators: WebAuthn authenticator flags are a wire bitmask.
  const hasCredential = (data[32] & 0x40) !== 0
  requireValue(
    data.length >= 87 && hasCredential && data.readUInt32BE(33) === 0 && data.readUInt16BE(53) === 32,
    'Invalid attestation authenticator data'
  )
  const aaguid =
    trust.environment === 'sandbox' ? Buffer.from('appattestdevelop') : Buffer.from('appattest\0\0\0\0\0\0\0')
  requireValue(data.subarray(37, 53).equals(aaguid), 'App Attest environment mismatch')
  const { cert, nonce } = credential(statement.x5c.map(bytes), trust.rootPem)
  requireValue(nonce.equals(hash(Buffer.concat([data, hash(clientData)]))), 'Nonce mismatch')
  const jwk = cert.publicKey.export({ format: 'jwk' })
  requireValue(jwk.kty === 'EC' && jwk.crv === 'P-256' && jwk.x && jwk.y, 'Expected P-256 key')
  const point = Buffer.concat([Buffer.from([4]), Buffer.from(jwk.x, 'base64url'), Buffer.from(jwk.y, 'base64url')])
  const identifier = Buffer.from(keyId, 'base64')
  requireValue(
    identifier.length === 32 && identifier.equals(hash(point)) && identifier.equals(data.subarray(55, 87)),
    'Key binding mismatch'
  )
  // Verify that the COSE key in authData also identifies the credential key.
  const decoded: unknown[] = []
  new Decoder({ mapsAsObjects: false, useRecords: false }).decodeMultiple(data.subarray(87), (value: unknown) => {
    decoded.push(value)
  })
  // biome-ignore lint/suspicious/noBitwiseOperators: WebAuthn extension presence is a wire bitmask.
  const hasExtensions = (data[32] & 0x80) !== 0
  requireValue(decoded.length === (hasExtensions ? 2 : 1), 'Invalid authenticator extensions')
  requireValue(hasExtensions || !trust.requireExtensions, 'Missing required authenticator extensions')
  if (hasExtensions) {
    validateExtensions(decoded[1], trust, false)
  }
  const cose = decoded[0]
  requireValue(
    cose instanceof Map &&
      cose.get(1) === 2 &&
      cose.get(3) === -7 &&
      cose.get(-1) === 1 &&
      bytes(cose.get(-2)).equals(point.subarray(1, 33)) &&
      bytes(cose.get(-3)).equals(point.subarray(33)),
    'COSE key mismatch'
  )
  return cert.publicKey.export({ type: 'spki', format: 'pem' }).toString()
}

export function assertion(
  encoded: string,
  publicKey: string,
  counter: number,
  clientData: Buffer,
  trust: Trust
): number {
  const value = object(encoded)
  const data = bytes(value.authenticatorData)
  rp(data, trust)
  // The entire authenticator data, including extensions, is signed below.
  // biome-ignore lint/suspicious/noBitwiseOperators: WebAuthn extension presence is a wire bitmask.
  const hasExtensions = (data[32] & 0x80) !== 0
  // biome-ignore lint/suspicious/noBitwiseOperators: Assertions cannot carry attested credential data.
  requireValue((data[32] & 0x40) === 0, 'Unexpected assertion credential data')
  requireValue(hasExtensions || !trust.requireExtensions, 'Missing required authenticator extensions')
  if (hasExtensions) {
    const values: unknown[] = []
    new Decoder({ mapsAsObjects: false, useRecords: false }).decodeMultiple(data.subarray(37), (value: unknown) => {
      values.push(value)
    })
    requireValue(values.length === 1, 'Invalid assertion extension framing')
    validateExtensions(values[0], trust, true)
  } else {
    requireValue(data.length === 37, 'Unexpected assertion trailing data')
  }
  const next = data.readUInt32BE(33)
  requireValue(next > counter, 'Replayed assertion')
  // ES256 verifies the SHA256 nonce by hashing authenticatorData || clientDataHash.
  requireValue(
    verify('sha256', Buffer.concat([data, hash(clientData)]), createPublicKey(publicKey), bytes(value.signature)),
    'Invalid assertion signature'
  )
  return next
}
