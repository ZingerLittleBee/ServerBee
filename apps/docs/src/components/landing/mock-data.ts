/* Mock fleet and product data behind the landing mocks. IPs use documentation ranges (RFC 5737). */

import type { LandingLang, UnlockStatus } from './translations'

type Localized = Record<LandingLang, string>

export type ServerId = 'hk' | 'tyo' | 'lax' | 'fra' | 'sin' | 'sjc'

/** Countries that mocks/flag.tsx can draw. */
export type CountryCode = 'de' | 'hk' | 'jp' | 'sg' | 'us'

export interface MockServer {
  cpu: number
  disk: number
  diskTxt: string
  /** Country of the flag drawn before the server name. */
  flag: CountryCode
  id: ServerId
  /** Network receive rate in MiB/s. */
  inb: number
  ip: string
  /** Latency baseline in ms, null while offline. */
  lat: number | null
  /** Latency jitter amplitude in ms. */
  latAmp: number
  load: number
  /** Packet loss level in percent, null while offline. */
  loss: number | null
  mem: number
  memTxt: string
  name: string
  online: boolean
  os: string
  /** Network transmit rate in MiB/s. */
  outb: number
  /** Memory size in GB as the detail headers show it; `memTxt` ends with the usable total the OS reports. */
  ramGb: number
  /** Disk read rate in MiB/s. */
  rd: number
  /** Latency level in ms of the occasional spikes on an unstable route. */
  spike?: number
  trf: number
  trfTxt: string
  /** vCPU count, shown in the detail headers. */
  vcpu: number
  /** Disk write rate in MiB/s. */
  wr: number
}

export const mockServers: MockServer[] = [
  {
    id: 'hk',
    flag: 'hk',
    name: 'HK · CN2 GIA',
    online: true,
    cpu: 23,
    load: 0.42,
    mem: 41,
    memTxt: '842 MB / 2.0 GB',
    disk: 38,
    diskTxt: '15.2 GB / 40.0 GB',
    trf: 27,
    trfTxt: '272 GB / 1.0 TB',
    inb: 4.8,
    outb: 1.2,
    rd: 0.3,
    wr: 1.1,
    lat: 34,
    latAmp: 4,
    loss: 0,
    ip: '203.0.*.*',
    os: 'Debian 13',
    vcpu: 2,
    ramGb: 2
  },
  {
    id: 'tyo',
    flag: 'jp',
    name: 'TYO · BGP',
    online: true,
    cpu: 12,
    load: 0.18,
    mem: 47,
    memTxt: '1.8 GB / 3.8 GB',
    disk: 22,
    diskTxt: '17.4 GB / 80.0 GB',
    trf: 64,
    trfTxt: '1.3 TB / 2.0 TB',
    inb: 18.6,
    outb: 22.4,
    rd: 0.1,
    wr: 0.6,
    lat: 52,
    latAmp: 5,
    loss: 0.3,
    ip: '198.51.*.*',
    os: 'Ubuntu 24.04',
    vcpu: 2,
    ramGb: 4
  },
  {
    id: 'lax',
    flag: 'us',
    name: 'LAX · 9929',
    online: true,
    cpu: 31,
    load: 0.77,
    mem: 58,
    memTxt: '2.2 GB / 3.8 GB',
    disk: 44,
    diskTxt: '26.4 GB / 60.0 GB',
    trf: 18,
    trfTxt: '184 GB / 1.0 TB',
    inb: 9.1,
    outb: 3.4,
    rd: 0,
    wr: 0.4,
    lat: 148,
    latAmp: 9,
    loss: 0,
    ip: '192.0.*.*',
    os: 'Debian 12',
    vcpu: 2,
    ramGb: 4
  },
  {
    id: 'fra',
    flag: 'de',
    name: 'FRA · Storage',
    online: true,
    cpu: 91,
    load: 3.64,
    mem: 76,
    memTxt: '6.1 GB / 8.0 GB',
    disk: 83,
    diskTxt: '830 GB / 1.0 TB',
    trf: 46,
    trfTxt: '4.6 TB / 10 TB',
    inb: 12.7,
    outb: 6.9,
    rd: 38.2,
    wr: 21.5,
    lat: 212,
    latAmp: 60,
    spike: 330,
    loss: 1.4,
    ip: '203.0.*.*',
    os: 'Debian 13',
    vcpu: 4,
    ramGb: 8
  },
  {
    id: 'sin',
    flag: 'sg',
    name: 'SIN · Premium',
    online: true,
    cpu: 17,
    load: 0.31,
    mem: 35,
    memTxt: '1.3 GB / 3.8 GB',
    disk: 29,
    diskTxt: '11.6 GB / 40.0 GB',
    trf: 12,
    trfTxt: '121 GB / 1.0 TB',
    inb: 3.3,
    outb: 2.8,
    rd: 0.2,
    wr: 0.5,
    lat: 71,
    latAmp: 6,
    loss: 0,
    ip: '203.0.*.*',
    os: 'Alpine 3.21',
    vcpu: 2,
    ramGb: 4
  },
  {
    id: 'sjc',
    flag: 'us',
    name: 'SJC · Edge',
    online: false,
    cpu: 0,
    load: 0,
    mem: 0,
    memTxt: '0 B / 1.9 GB',
    disk: 0,
    diskTxt: '0 B / 30.0 GB',
    trf: 0,
    trfTxt: '0 B / 1.0 TB',
    inb: 0,
    outb: 0,
    rd: 0,
    wr: 0,
    lat: null,
    latAmp: 0,
    loss: null,
    ip: '192.0.*.*',
    os: 'Debian 12',
    vcpu: 1,
    ramGb: 2
  }
]

/** The mock server with this id, for mocks that show one server's detail screen. */
export function mockServer(id: ServerId): MockServer {
  const server = mockServers.find((candidate) => candidate.id === id)
  if (!server) {
    throw new Error(`Unknown mock server: ${id}`)
  }
  return server
}

/** Carrier ids in `ProductCopy.provNames` order: China Telecom, China Unicom, China Mobile. */
export const carrierIds = ['ct', 'cu', 'cm'] as const

export type CarrierId = (typeof carrierIds)[number]

/** Index into `carrierIds` and `ProductCopy.provNames`. */
type CarrierIndex = 0 | 1 | 2

interface ProbeTarget {
  /** Latency noise amplitude in ms. */
  amp: number
  /** Latency baseline in ms. */
  base: number
  /** Chart series color, following the product's chart palette. */
  color: string
  id: string
  isp: CarrierIndex
  name: Localized
  /** Height in ms of the sharp latency spike that repeats every `spikeEvery` samples. */
  spike?: number
  /** Samples between two latency spikes. */
  spikeEvery?: number
}

/** HK · CN2 GIA probe targets: 6 of the 96 presets. */
export const probeTargets: ProbeTarget[] = [
  {
    id: 'sh-cm',
    name: { zh: '上海移动', en: 'Shanghai Mobile' },
    isp: 2,
    base: 35,
    amp: 1.6,
    color: 'oklch(0.623 0.214 259.8)',
    spikeEvery: 29,
    spike: 88
  },
  {
    id: 'sh-ct',
    name: { zh: '上海电信', en: 'Shanghai Telecom' },
    isp: 0,
    base: 32,
    amp: 1.2,
    color: 'oklch(0.637 0.237 25.3)'
  },
  {
    id: 'sh-cu',
    name: { zh: '上海联通', en: 'Shanghai Unicom' },
    isp: 1,
    base: 31.3,
    amp: 1.1,
    color: 'oklch(0.723 0.219 149.6)'
  },
  {
    id: 'gd-cm',
    name: { zh: '广东移动', en: 'Guangdong Mobile' },
    isp: 2,
    base: 22.4,
    amp: 1.8,
    color: 'oklch(0.769 0.188 70.1)'
  },
  {
    id: 'gd-ct',
    name: { zh: '广东电信', en: 'Guangdong Telecom' },
    isp: 0,
    base: 18.9,
    amp: 1,
    color: 'oklch(0.606 0.25 292.7)'
  },
  {
    id: 'gd-cu',
    name: { zh: '广东联通', en: 'Guangdong Unicom' },
    isp: 1,
    base: 20.6,
    amp: 1.3,
    color: 'oklch(0.656 0.241 354.3)',
    spikeEvery: 37,
    spike: 54
  }
]

type RiskTone = 'low' | 'mid' | 'high'

interface IpCardData {
  flags: { id: string; label: Localized }[]
  id: ServerId
  ip: string
  level: Localized
  loc: string
  name: string
  score: number
  tone: RiskTone
  type: Localized
}

const hostingFlag = { id: 'hosting', label: { zh: '主机托管', en: 'Hosting' } }

/** Risk score of the TYO · BGP egress IP, on its IP card and on the iOS IP-quality screen. */
export const TYO_RISK_SCORE = 38

export const ipCards: IpCardData[] = [
  {
    id: 'tyo',
    name: 'TYO · BGP',
    ip: '198.51.100.24',
    type: { zh: '数据中心', en: 'Datacenter' },
    score: TYO_RISK_SCORE,
    level: { zh: '中', en: 'medium' },
    tone: 'mid',
    flags: [hostingFlag],
    loc: 'JP'
  },
  {
    id: 'sin',
    name: 'SIN · Premium',
    ip: '203.0.113.87',
    type: { zh: 'ISP', en: 'ISP' },
    score: 12,
    level: { zh: '低', en: 'low' },
    tone: 'low',
    flags: [],
    loc: 'SG'
  },
  {
    id: 'lax',
    name: 'LAX · 9929',
    ip: '192.0.2.61',
    type: { zh: '数据中心', en: 'Datacenter' },
    score: 71,
    level: { zh: '高', en: 'high' },
    tone: 'high',
    flags: [hostingFlag, { id: 'abuser', label: { zh: '已知滥用', en: 'Known abuser' } }],
    loc: 'US'
  }
]

export type ServiceId = 'netflix' | 'disney' | 'youtube' | 'prime' | 'hbo' | 'chatgpt' | 'gemini' | 'tiktok' | 'spotify'

/** Unlock-matrix columns, grouped as streaming (5), AI (2) and social (2). */
export const unlockServices: { id: ServiceId; name: string }[] = [
  { id: 'netflix', name: 'Netflix' },
  { id: 'disney', name: 'Disney+' },
  { id: 'youtube', name: 'YouTube Premium' },
  { id: 'prime', name: 'Amazon Prime Video' },
  { id: 'hbo', name: 'HBO Max' },
  { id: 'chatgpt', name: 'ChatGPT' },
  { id: 'gemini', name: 'Google Gemini' },
  { id: 'tiktok', name: 'TikTok' },
  { id: 'spotify', name: 'Spotify' }
]

export type UnlockCell = UnlockStatus | 'none'

interface UnlockRowData {
  /** One state per service, in `unlockServices` order. */
  cells: UnlockCell[]
  id: ServerId
  name: string
}

export const unlockRows: UnlockRowData[] = [
  { id: 'tyo', name: 'TYO · BGP', cells: ['ok', 'ok', 'ok', 'ok', 'blk', 'ok', 'ok', 'ok', 'ok'] },
  { id: 'lax', name: 'LAX · 9929', cells: ['ok', 'ok', 'fail', 'ok', 'ok', 'ok', 'ok', 'ok', 'ok'] },
  { id: 'sin', name: 'SIN · Premium', cells: ['lim', 'ok', 'ok', 'ok', 'ok', 'ok', 'ok', 'ok', 'ok'] },
  { id: 'fra', name: 'FRA · Storage', cells: ['ok', 'ok', 'ok', 'ok', 'blk', 'ok', 'ok', 'ok', 'ok'] },
  { id: 'sjc', name: 'SJC · Edge', cells: ['none', 'none', 'none', 'none', 'none', 'none', 'none', 'none', 'none'] }
]

export type AccessCategory = 'ai' | 'stream' | 'social'

interface PhoneAccessGroup {
  cat: AccessCategory
  rows: { id: ServiceId; name: string; st: 'ok' | 'blk'; region?: string }[]
}

/** Service access of TYO · BGP on the iOS IP-quality screen. */
export const phoneAccess: PhoneAccessGroup[] = [
  {
    cat: 'ai',
    rows: [
      { id: 'chatgpt', name: 'ChatGPT', st: 'ok', region: 'JP' },
      { id: 'gemini', name: 'Google Gemini', st: 'ok' }
    ]
  },
  {
    cat: 'stream',
    rows: [
      { id: 'netflix', name: 'Netflix', st: 'ok' },
      { id: 'disney', name: 'Disney+', st: 'ok' },
      { id: 'youtube', name: 'YouTube Premium', st: 'ok' },
      { id: 'prime', name: 'Amazon Prime Video', st: 'ok' },
      { id: 'hbo', name: 'HBO Max', st: 'blk' }
    ]
  },
  {
    cat: 'social',
    rows: [
      { id: 'tiktok', name: 'TikTok', st: 'ok' },
      { id: 'spotify', name: 'Spotify', st: 'ok' }
    ]
  }
]

interface CpuSeries {
  amp: number
  base: number
  color: string
  id: ServerId
  name: string
  spike?: number
  spikeEvery?: number
}

/** Dashboard "CPU comparison" widget series. */
export const cpuCompare: CpuSeries[] = [
  { id: 'hk', name: 'HK · CN2 GIA', base: 23, amp: 3, color: 'oklch(0.623 0.214 259.8)' },
  { id: 'tyo', name: 'TYO · BGP', base: 12, amp: 2, color: 'oklch(0.723 0.219 149.6)' },
  { id: 'lax', name: 'LAX · 9929', base: 31, amp: 4, color: 'oklch(0.705 0.213 47.6)', spikeEvery: 23, spike: 14 }
]

/** Dashboard uptime timeline rows; `bad` is the share of degraded days, a quarter of them down. */
export const uptimeRows: { id: ServerId; name: string; seed: number; bad: number }[] = [
  { id: 'hk', name: 'HK · CN2 GIA', seed: 1, bad: 0 },
  { id: 'tyo', name: 'TYO · BGP', seed: 2, bad: 0.02 },
  { id: 'fra', name: 'FRA · Storage', seed: 3, bad: 0.08 }
]

/** Daily fleet traffic in GB, drawn against a 900 GB scale. */
export const trafficDays: { id: string; inbound: number; outbound: number }[] = [
  { id: '09-24', inbound: 412, outbound: 238 },
  { id: '09-25', inbound: 388, outbound: 251 },
  { id: '09-26', inbound: 455, outbound: 302 },
  { id: '09-27', inbound: 398, outbound: 260 },
  { id: '09-28', inbound: 502, outbound: 331 },
  { id: '09-29', inbound: 476, outbound: 298 },
  { id: '09-30', inbound: 289, outbound: 172 }
]

export const TRAFFIC_SCALE_GB = 900

/** Agents in the "How it works" diagram. */
export const archAgents: { id: ServerId; name: string; os: string }[] = [
  { id: 'hk', name: 'HK · CN2 GIA', os: 'linux/amd64' },
  { id: 'tyo', name: 'TYO · BGP', os: 'linux/arm64' },
  { id: 'fra', name: 'FRA · Storage', os: 'linux/amd64' }
]

/** The server sends push notifications in English, so the title is not localized. */
export const PUSH_TITLE = '[ServerBee] FRA · Storage triggered'
