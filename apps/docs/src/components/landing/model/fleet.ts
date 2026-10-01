/* Per-server card values for tick t. The hero cards, the dashboard and the iOS list all read these,
   so every mock shows the same fleet. */

import type { CSSProperties } from 'react'

import { type CountryCode, type MockServer, mockServers, type ServerId } from '../mock-data'
import { type LandingLang, landingCopy, type ProductCopy } from '../translations'
import { clamp, f1, jitter, latencyTone, lossTone, noise, usageColor } from './shared'

/** Number of time buckets in each latency and loss strip. */
const STRIP_BUCKETS = 30

interface Ring {
  id: 'cpu' | 'mem' | 'disk' | 'trf'
  label: string
  /** Sets `--v` (fill level) and `--c` (fill color) for the `.sv-dial` element. */
  style: CSSProperties
  sub: string
  val: string
}

interface Rate {
  id: 'in' | 'out' | 'read' | 'write'
  /** Bold key ("↓ 入站", "R") and the optional muted key after it ("读"). */
  k: string
  k2: string
  /** Rate in MiB/s. */
  raw: number
  u: string
  v: string
}

export interface StripCell {
  /** `cell ok|warn|sev|fail|none`. */
  className: string
  /** The absolute time bucket, so a cell keeps its key while the strip scrolls. */
  id: string
}

export interface ServerCard {
  /** `sv-card`, or `sv-card is-off` while offline. */
  className: string
  cpu: number
  cpuRaw: number
  disk: number
  flag: CountryCode
  id: ServerId
  ip: string
  latCells: StripCell[]
  /** `sv-val t-ok|warn|none`. */
  latClassName: string
  latTxt: string
  lossCells: StripCell[]
  /** `sv-val t-ok|warn|sev|fail|none`. */
  lossClassName: string
  lossTxt: string
  mem: number
  memRaw: number
  name: string
  net: Rate[]
  online: boolean
  os: string
  /** `sv-pill on|off`. */
  pillClassName: string
  rings: Ring[]
  status: string
}

/** Latency (ms) of one strip bucket. */
function latencyAt(server: MockServer, index: number, bucket: number): number | null {
  if (!server.online || server.lat === null) {
    return null
  }
  if (server.spike && noise(4, index, bucket) > 0.87) {
    return server.spike + noise(5, 0, bucket) * 40
  }
  return server.lat + (noise(3, index, bucket) - 0.5) * 2 * server.latAmp
}

/** Packet loss (%) of one strip bucket. */
function lossAt(server: MockServer, index: number, bucket: number): number | null {
  if (!server.online || server.loss === null) {
    return null
  }
  if (!server.loss) {
    return 0
  }
  const h = noise(6, index, bucket)
  if (server.loss < 1) {
    return h > 0.9 ? 0.6 : 0
  }
  if (h > 0.95) {
    return 6.2
  }
  return h > 0.72 ? 1 + h * 2 : 0
}

/** Formats a rate like the product's MetricValue: "0" when idle, KB/s below 1 MB/s. */
function rate(id: Rate['id'], k: string, k2: string, mb: number): Rate {
  if (mb <= 0) {
    return { id, k, k2, raw: mb, v: '0', u: '' }
  }
  if (mb < 1) {
    return { id, k, k2, raw: mb, v: (mb * 1024).toFixed(1), u: 'KB/s' }
  }
  return { id, k, k2, raw: mb, v: mb.toFixed(1), u: 'MB/s' }
}

function serverCard(server: MockServer, index: number, tick: number, p: ProductCopy): ServerCard {
  const on = server.online
  const cpuRaw = on ? clamp(server.cpu + jitter(index + 1, tick, server.cpu > 80 ? 3 : 4), 1, 99) : 0
  const memRaw = on ? clamp(server.mem + jitter(index + 11, tick, 1), 1, 99) : 0
  const cpu = Math.round(cpuRaw)
  const mem = Math.round(memRaw)

  const latCells: StripCell[] = []
  const lossCells: StripCell[] = []
  for (let offset = 0; offset < STRIP_BUCKETS; offset += 1) {
    const bucket = tick + offset
    latCells.push({ id: String(bucket), className: `cell ${latencyTone(latencyAt(server, index, bucket))}` })
    lossCells.push({ id: String(bucket), className: `cell ${lossTone(lossAt(server, index, bucket))}` })
  }
  const lastBucket = tick + STRIP_BUCKETS - 1
  const lat = latencyAt(server, index, lastBucket)
  const loss = lossAt(server, index, lastBucket)

  const io = (value: number, seed: number) =>
    on && value > 0 ? Math.max(0, value + jitter(index + seed, tick, Math.max(0.1, value * 0.18))) : 0
  const ring = (id: Ring['id'], label: string, sub: string, value: number): Ring => ({
    id,
    label,
    sub,
    val: String(value),
    style: { '--v': String(value), '--c': on ? usageColor(value) : 'var(--p-track)' }
  })
  const load = on ? (server.load + jitter(index + 21, tick, 0.04)).toFixed(2) : '0.00'

  return {
    id: server.id,
    name: server.name,
    flag: server.flag,
    online: on,
    className: on ? 'sv-card' : 'sv-card is-off',
    pillClassName: on ? 'sv-pill on' : 'sv-pill off',
    status: on ? p.online : p.offline,
    rings: [
      ring('cpu', 'CPU', `${p.load} ${load}`, cpu),
      ring('mem', p.mem, server.memTxt, mem),
      ring('disk', p.disk, server.diskTxt, server.disk),
      ring('trf', p.traffic, server.trfTxt, server.trf)
    ],
    net: [
      rate('in', p.netIn, '', io(server.inb, 31)),
      rate('out', p.netOut, '', io(server.outb, 41)),
      rate('read', 'R', p.read, io(server.rd, 51)),
      rate('write', 'W', p.write, io(server.wr, 61))
    ],
    latTxt: lat === null ? '—' : String(Math.round(lat)),
    latClassName: `sv-val t-${latencyTone(lat)}`,
    lossTxt: loss === null ? '—' : `${f1(loss)}%`,
    lossClassName: `sv-val t-${lossTone(loss)}`,
    latCells,
    lossCells,
    cpu,
    mem,
    cpuRaw,
    memRaw,
    disk: server.disk,
    ip: server.ip,
    os: server.os
  }
}

/** The web panel's server header facts: OS, vCPU count and memory ("Debian 13", "2 vCPU", "2.0 GB"). */
export function webSpecs(server: MockServer): string[] {
  return [server.os, `${server.vcpu} vCPU`, `${f1(server.ramGb)} GB`]
}

/** The iOS detail chip, "2 vCPU · 4 GB": like the app's byte formatter, a whole number drops its decimal. */
export function iosSpecs(server: MockServer): string {
  return `${server.vcpu} vCPU · ${Math.round(server.ramGb * 10) / 10} GB`
}

/** All six mock servers at `tick`, in `mockServers` order (the last one, SJC · Edge, is offline). */
export function buildServerCards(lang: LandingLang, tick: number): ServerCard[] {
  const p = landingCopy[lang].p
  return mockServers.map((server, index) => serverCard(server, index, tick, p))
}
