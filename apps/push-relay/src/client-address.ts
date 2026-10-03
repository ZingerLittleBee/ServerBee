import { isIP } from 'node:net'
import type { Relay } from './relay'

const clientIPHeader = 'x-serverbee-client-ip'
const mappedIPv4 = /^::ffff:([a-f0-9]+):([a-f0-9]+)$/

/** A bare IP only: no ports, brackets, zone IDs or lists. */
export function normalizeIP(value: string): string | null {
  if (value.length > 45 || value.includes('%')) {
    return null
  }
  const family = isIP(value)
  if (family === 4) {
    return value
  }
  if (family !== 6) {
    return null
  }
  const canonical = new URL(`http://[${value}]/`).hostname.slice(1, -1)
  // Socket APIs may represent the same IPv4 peer as an IPv4-mapped IPv6 address.
  const mapped = mappedIPv4.exec(canonical)
  if (mapped) {
    const high = Number.parseInt(mapped[1], 16)
    const low = Number.parseInt(mapped[2], 16)
    return [Math.floor(high / 256), high % 256, Math.floor(low / 256), low % 256].join('.')
  }
  return canonical
}

export function parseTrustedProxyIPs(value: string): ReadonlySet<string> {
  const addresses = value.split(',').map((raw) => normalizeIP(raw.trim()))
  if (addresses.some((address) => address === null)) {
    throw new Error('RELAY_TRUSTED_PROXY_IPS requires a comma-separated list of bare proxy IPs')
  }
  return new Set(addresses as string[])
}

interface PeerServer {
  requestIP(request: Request): { address: string } | null
}

/** Keep the native peer lookup and trusted-header boundary in the production seam. */
export function createRelayFetch(relay: Pick<Relay, 'handle'>, trustedProxies: ReadonlySet<string>) {
  return (request: Request, server: PeerServer): Promise<Response> | Response => {
    const peer = normalizeIP(server.requestIP(request)?.address ?? '')
    // Reject any observable list. Bun can discard repeated custom wire fields,
    // so trusted proxies MUST overwrite this header rather than pass it through.
    const source = peer && trustedProxies.has(peer) ? normalizeIP(request.headers.get(clientIPHeader) ?? '') : peer
    if (!source) {
      return Response.json({ error: 'Invalid client address' }, { status: 400 })
    }
    // An untrusted peer's Forwarded/X-Forwarded-For/custom headers are irrelevant.
    return relay.handle(request, source)
  }
}
