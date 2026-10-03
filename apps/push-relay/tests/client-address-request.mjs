// Run with real Node: Bun's node:http compatibility client does not preserve
// localAddress, so it cannot establish distinct native downstream socket peers.
import { request } from 'node:http'

const options = JSON.parse(process.argv[2])
const body = options.body === undefined ? undefined : JSON.stringify(options.body)
const pending = request(
  options.url,
  {
    localAddress: options.localAddress,
    agent: false,
    method: options.method ?? (body === undefined ? 'GET' : 'POST'),
    headers: options.headers
  },
  (response) => {
    const chunks = []
    response.on('data', (chunk) => chunks.push(chunk))
    response.on('error', fail)
    response.on('end', () => {
      process.stdout.write(JSON.stringify({ status: response.statusCode, body: Buffer.concat(chunks).toString() }))
    })
  }
)

function fail(error) {
  process.stderr.write(error.message)
  process.exitCode = 1
}

pending.on('error', fail)
pending.setTimeout(5000, () => pending.destroy(new Error('Local fixture request timed out')))
pending.end(body)
