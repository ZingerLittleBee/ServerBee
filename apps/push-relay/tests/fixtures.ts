/** Generated Apple-format fixtures with an isolated test CA. Never real device
 * proof; production code still verifies the full certificate path and nonce. */
import { execFileSync } from 'node:child_process'
import { createPublicKey, sign } from 'node:crypto'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Encoder } from 'cbor-x'
import { type Environment, hash } from '../src/attestation'

const encoder = new Encoder({ useRecords: false })
const appId = 'TESTTEAM01.com.serverbee.mobile'

export class AppleFixture {
  readonly directory = mkdtempSync(join(tmpdir(), 'serverbee-apple-fixture-'))
  readonly rootPem: string
  readonly keyId: string
  readonly point: Buffer
  readonly privateKey: string

  constructor(authority?: AppleFixture) {
    this.openssl([
      'req',
      '-x509',
      '-newkey',
      'ec',
      '-pkeyopt',
      'ec_paramgen_curve:P-256',
      '-nodes',
      '-keyout',
      'root.key',
      '-out',
      'root.pem',
      '-days',
      '2',
      '-subj',
      '/CN=Isolated App Attest Fixture Root',
      '-addext',
      'basicConstraints=critical,CA:TRUE,pathlen:1'
    ])
    this.openssl([
      'req',
      '-new',
      '-newkey',
      'ec',
      '-pkeyopt',
      'ec_paramgen_curve:P-256',
      '-nodes',
      '-keyout',
      'intermediate.key',
      '-out',
      'intermediate.csr',
      '-subj',
      '/CN=Fixture App Attest Intermediate'
    ])
    writeFileSync(
      join(this.directory, 'ca.ext'),
      'basicConstraints=critical,CA:TRUE,pathlen:0\nkeyUsage=critical,keyCertSign,cRLSign\n'
    )
    this.openssl([
      'x509',
      '-req',
      '-in',
      'intermediate.csr',
      '-CA',
      'root.pem',
      '-CAkey',
      'root.key',
      '-CAcreateserial',
      '-out',
      'intermediate.pem',
      '-days',
      '2',
      '-extfile',
      'ca.ext'
    ])
    if (authority) {
      for (const file of ['root.key', 'root.pem', 'intermediate.key', 'intermediate.pem']) {
        writeFileSync(join(this.directory, file), readFileSync(join(authority.directory, file)))
      }
    }
    this.openssl([
      'req',
      '-new',
      '-newkey',
      'ec',
      '-pkeyopt',
      'ec_paramgen_curve:P-256',
      '-nodes',
      '-keyout',
      'leaf.key',
      '-out',
      'leaf.csr',
      '-subj',
      '/CN=Fixture App Attest Key'
    ])
    this.rootPem = readFileSync(join(this.directory, 'root.pem'), 'utf8')
    this.privateKey = readFileSync(join(this.directory, 'leaf.key'), 'utf8')
    const key = createPublicKey(this.privateKey).export({ format: 'jwk' })
    this.point = Buffer.concat([
      Buffer.from([4]),
      Buffer.from(key.x ?? '', 'base64url'),
      Buffer.from(key.y ?? '', 'base64url')
    ])
    this.keyId = hash(this.point).toString('base64')
  }

  trust(environment: Environment = 'sandbox') {
    return { appId, rootPem: this.rootPem, environment, bundleVersions: ['1.0'] }
  }
  close() {
    rmSync(this.directory, { recursive: true, force: true })
  }

  private openssl(args: string[]) {
    return execFileSync('openssl', args, { cwd: this.directory, stdio: 'pipe', timeout: 5000 })
  }

  attestation(
    clientData: Buffer,
    options: {
      wrongNonce?: boolean
      wrongApp?: boolean
      environment?: Environment
      wrongCredential?: boolean
      wrongCose?: boolean
      expired?: boolean
      extensions?: Map<string, unknown>
    } = {}
  ): string {
    const data = Buffer.alloc(87)
    hash(options.wrongApp ? 'wrong.app' : appId).copy(data)
    data[32] = options.extensions ? 0xc1 : 0x41
    Buffer.from(options.environment === 'production' ? 'appattest\0\0\0\0\0\0\0' : 'appattestdevelop').copy(data, 37)
    data.writeUInt16BE(32, 53)
    Buffer.from(this.keyId, 'base64').copy(data, 55)
    if (options.wrongCredential) {
      data[55] = (data[55] + 1) % 256
    }
    const cose = encoder.encode(
      new Map<number, unknown>([
        [1, 2],
        [3, -7],
        [-1, 1],
        [-2, options.wrongCose ? Buffer.alloc(32) : this.point.subarray(1, 33)],
        [-3, this.point.subarray(33)]
      ])
    )
    const authData = Buffer.concat([data, cose, ...(options.extensions ? [encoder.encode(options.extensions)] : [])])
    const nonce = hash(Buffer.concat([authData, hash(clientData)]))
    if (options.wrongNonce) {
      nonce[0] = (nonce[0] + 1) % 256
    }
    const hex = Buffer.concat([Buffer.from('3024a1220420', 'hex'), nonce])
      .toString('hex')
      .match(/../g)
      ?.join(':')
    writeFileSync(
      join(this.directory, 'leaf.ext'),
      `basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\n1.2.840.113635.100.8.2=DER:${hex}\n`
    )
    if (options.expired) {
      // Explicit historic dates work across OpenSSL 3 versions; negative
      // -days is rejected before any certificate can reach the handler.
      writeFileSync(join(this.directory, 'index.txt'), '')
      writeFileSync(join(this.directory, 'serial.txt'), '01\n')
      writeFileSync(
        join(this.directory, 'issuer.cnf'),
        `
[ca]
default_ca = fixture
[fixture]
database = index.txt
serial = serial.txt
new_certs_dir = .
certificate = intermediate.pem
private_key = intermediate.key
default_md = sha256
policy = fixture_policy
[fixture_policy]
commonName = supplied
`
      )
      this.openssl([
        'ca',
        '-batch',
        '-config',
        'issuer.cnf',
        '-in',
        'leaf.csr',
        '-out',
        'leaf.pem',
        '-startdate',
        '20200101000000Z',
        '-enddate',
        '20200102000000Z',
        '-extfile',
        'leaf.ext'
      ])
    } else {
      this.openssl([
        'x509',
        '-req',
        '-in',
        'leaf.csr',
        '-CA',
        'intermediate.pem',
        '-CAkey',
        'intermediate.key',
        '-CAcreateserial',
        '-out',
        'leaf.pem',
        '-days',
        '2',
        '-extfile',
        'leaf.ext'
      ])
    }
    const leaf = this.openssl(['x509', '-in', 'leaf.pem', '-outform', 'DER'])
    const intermediate = this.openssl(['x509', '-in', 'intermediate.pem', '-outform', 'DER'])
    return encoder
      .encode({
        fmt: 'apple-appattest',
        attStmt: { x5c: [leaf, intermediate], receipt: Buffer.from('isolated-receipt') },
        authData
      })
      .toString('base64')
  }

  assertion(
    clientData: Buffer,
    counter: number,
    wrongSignature = false,
    options: {
      extensions?: Map<string, unknown>
      trailing?: Buffer
      omitExtensionFlag?: boolean
      tamperExtensions?: boolean
    } = {}
  ): string {
    const header = Buffer.alloc(37)
    hash(appId).copy(header)
    header[32] = options.extensions && !options.omitExtensionFlag ? 0x81 : 1
    header.writeUInt32BE(counter, 33)
    const authenticatorData = Buffer.concat([
      header,
      ...(options.extensions ? [encoder.encode(options.extensions)] : []),
      options.trailing ?? Buffer.alloc(0)
    ])
    const signature = sign('sha256', Buffer.concat([authenticatorData, hash(clientData)]), this.privateKey)
    if (wrongSignature) {
      signature[0] = (signature[0] + 1) % 256
    }
    if (options.tamperExtensions) {
      authenticatorData[authenticatorData.length - 1] = ((authenticatorData.at(-1) ?? 0) + 1) % 256
    }
    return encoder.encode({ signature, authenticatorData }).toString('base64')
  }
}
