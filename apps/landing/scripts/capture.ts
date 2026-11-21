/*
 * Captures the product screenshots the landing page shows, from a local demo server.
 *
 *   SERVERBEE_RATE_LIMIT__LOGIN_MAX=1000 scripts/server-dev-demo.sh   # terminal 1, after `bun run build:web`
 *   bun run capture                                                    # terminal 2, in apps/landing
 *
 * The demo server seeds twelve servers with live metrics. This script renames them and builds one dashboard through
 * the real API, so those writes only reach the throwaway demo database. Data the seed does not produce (carrier
 * probe targets, IP quality results, a temporary capability grant, a quiet uptime history) is supplied to the page
 * as fixture responses, and the real web UI renders it. Every run signs in once; the raised login limit keeps
 * repeated runs from being rate limited.
 *
 * Output: src/assets/shots/{name}-{lang}-{theme}.webp, lossless. `--only=<name>` captures a single shot.
 */

import { mkdirSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { type BrowserContext, chromium, type Page, type Route } from 'playwright'
import sharp from 'sharp'

const BASE = process.env.SERVERBEE_DEMO_URL ?? 'http://localhost:9527'
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SHOTS = join(ROOT, 'src/assets/shots')

const LANGS = ['en', 'zh'] as const
const THEMES = ['light', 'dark'] as const
type Lang = (typeof LANGS)[number]
type Theme = (typeof THEMES)[number]

const only = process.argv.find((arg) => arg.startsWith('--only='))?.slice('--only='.length)

/**
 * The fleet shown in every shot, by the id suffix the demo seed assigns: display name, monthly price, traffic
 * allowance and the share of it used, and the latency its probe strip centers on.
 */
const FLEET: Record<string, { latency: number; limitTb: number; name: string; price: number; used: number }> = {
  '01': { name: 'SJC · Edge', price: 6.5, limitTb: 2, used: 0.31, latency: 127 },
  '02': { name: 'LAX · 9929', price: 9.9, limitTb: 1, used: 0.27, latency: 132 },
  '03': { name: 'TYO · BGP', price: 7.0, limitTb: 2, used: 0.64, latency: 48 },
  '04': { name: 'SIN · Premium', price: 8.8, limitTb: 1, used: 0.42, latency: 62 },
  '05': { name: 'FRA · Storage', price: 5.5, limitTb: 4, used: 0.18, latency: 192 },
  '06': { name: 'AMS · Core', price: 9.0, limitTb: 4, used: 0.36, latency: 178 },
  '07': { name: 'HK · CN2 GIA', price: 9.5, limitTb: 1, used: 0.58, latency: 34 },
  '08': { name: 'SYD · Edge', price: 6.0, limitTb: 2, used: 0.22, latency: 118 },
  '09': { name: 'NYC · Lab', price: 4.5, limitTb: 3, used: 0.47, latency: 171 },
  '10': { name: 'YYZ · Lab', price: 3.5, limitTb: 2, used: 0.13, latency: 164 },
  '11': { name: 'LON · Backup', price: 4.0, limitTb: 3, used: 0.71, latency: 186 },
  '12': { name: 'BLR · Dev', price: 7.5, limitTb: 1, used: 0.39, latency: 88 }
}
const GROUPS: Record<string, string> = { 'dev-demo-edge': 'Edge', 'dev-demo-core': 'Core', 'dev-demo-lab': 'Lab' }
const FOCUS = 'dev-demo-07'
const TB = 1024 ** 4

const DASHBOARD_NAME = 'Ops overview'
const RUNBOOK = [
  '### On call',
  '',
  '1. CPU alert: check **FRA · Storage** first',
  '2. Loss over 5%: call upstream'
].join('\n')

interface Json {
  [key: string]: unknown
}

async function api<T = unknown>(page: Page, method: string, path: string, body?: unknown): Promise<T> {
  const result = await page.evaluate(
    async ([m, p, b]) => {
      const response = await fetch(`/api${p}`, {
        method: m as string,
        headers: b === undefined ? undefined : { 'Content-Type': 'application/json' },
        body: b === undefined ? undefined : JSON.stringify(b)
      })
      const text = await response.text()
      return { status: response.status, text }
    },
    [method, path, body] as const
  )
  if (result.status >= 400) {
    throw new Error(`${method} ${path} answered ${result.status}: ${result.text.slice(0, 200)}`)
  }
  return (result.text ? JSON.parse(result.text).data : undefined) as T
}

/** Renames the seeded fleet and rebuilds the showcase dashboard. Returns the dashboard id. */
async function stage(page: Page): Promise<string> {
  for (const [suffix, spec] of Object.entries(FLEET)) {
    await api(page, 'PUT', `/servers/dev-demo-${suffix}`, {
      name: spec.name,
      price: spec.price,
      traffic_limit: Math.round(spec.limitTb * TB),
      traffic_limit_type: 'sum'
    })
  }
  for (const [id, name] of Object.entries(GROUPS)) {
    await api(page, 'PUT', `/server-groups/${id}`, { name })
  }
  const dashboards = await api<{ id: string; name: string }[]>(page, 'GET', '/dashboards')
  // A rerun rewrites the dashboard it made earlier: the default dashboard cannot be deleted.
  const created =
    dashboards.find((dashboard) => dashboard.name === DASHBOARD_NAME) ??
    (await api<{ id: string }>(page, 'POST', '/dashboards', { name: DASHBOARD_NAME }))
  const id = (n: string) => `dev-demo-${n}`
  const widgets = [
    { widget_type: 'stat-number', config: { metric: 'server_count' }, x: 0, y: 0, w: 3, h: 1 },
    { widget_type: 'stat-number', config: { metric: 'avg_cpu' }, x: 3, y: 0, w: 3, h: 1 },
    { widget_type: 'stat-number', config: { metric: 'avg_memory' }, x: 6, y: 0, w: 3, h: 1 },
    { widget_type: 'stat-number', config: { metric: 'total_bandwidth' }, x: 9, y: 0, w: 3, h: 1 },
    {
      widget_type: 'multi-line',
      config: { metric: 'cpu', hours: 6, server_ids: [id('07'), id('03'), id('02'), id('05')] },
      x: 0,
      y: 1,
      w: 6,
      h: 4
    },
    { widget_type: 'gauge', config: { metric: 'cpu', server_id: id('05') }, x: 6, y: 1, w: 2, h: 2 },
    { widget_type: 'gauge', config: { metric: 'memory', server_id: id('03') }, x: 6, y: 3, w: 2, h: 2 },
    { widget_type: 'top-n', config: { metric: 'memory', count: 5 }, x: 8, y: 1, w: 4, h: 4 },
    { widget_type: 'server-map', config: {}, x: 0, y: 5, w: 4, h: 4 },
    { widget_type: 'traffic-bar', config: { hours: 168, server_id: id('07') }, x: 4, y: 5, w: 4, h: 4 },
    { widget_type: 'uptime-timeline', config: { days: 90, server_ids: [id('07'), id('03')] }, x: 8, y: 5, w: 4, h: 2 },
    { widget_type: 'markdown', config: { content: RUNBOOK }, x: 8, y: 7, w: 4, h: 2 }
  ]
  await api(page, 'PUT', `/dashboards/${created.id}`, {
    widgets: widgets.map((widget, index) => ({
      widget_type: widget.widget_type,
      config_json: widget.config,
      grid_x: widget.x,
      grid_y: widget.y,
      grid_w: widget.w,
      grid_h: widget.h,
      sort_order: index
    }))
  })
  return created.id
}

/* ---------------------------------------------------------------- fixtures */

/** Demo probe target id, the preset it stands in for, and that preset's latency relative to the demo series. */
const CARRIER_TARGETS = [
  { from: 'dev-demo-cloudflare', to: 'cn-sh-ct', scale: 0.56, shift: 0 },
  { from: 'dev-demo-google', to: 'cn-sh-cu', scale: 0.5, shift: 0 },
  { from: 'dev-demo-tokyo', to: 'cn-sh-cm', scale: 0.37, shift: 0 },
  { from: 'dev-demo-cloudflare', to: 'cn-gd-ct', scale: 0.31, shift: 5 },
  { from: 'dev-demo-google', to: 'cn-gd-cu', scale: 0.34, shift: 9 },
  { from: 'dev-demo-tokyo', to: 'cn-gd-cm', scale: 0.24, shift: 3 }
] as const

interface ProbeTarget extends Json {
  id: string
  name: string
  provider: string
}

function round1(value: number): number {
  return Math.round(value * 10) / 10
}

function scaleLatency<T extends Json>(row: T, scale: number): T {
  const scaled: Json = { ...row }
  for (const key of ['avg_latency', 'min_latency', 'max_latency']) {
    if (typeof row[key] === 'number') {
      scaled[key] = round1((row[key] as number) * scale)
    }
  }
  return scaled as T
}

/** Rewrites the focus server's probe responses so its three demo targets read as six carrier presets. */
function carrierFixtures(presets: ProbeTarget[]) {
  const preset = (id: string) => {
    const found = presets.find((target) => target.id === id)
    if (!found) {
      throw new Error(`Probe preset ${id} is missing from /api/network-probes/targets`)
    }
    return found
  }
  return {
    summary(data: Json): Json {
      const targets = data.targets as Json[]
      return {
        ...data,
        anomaly_count: 2,
        targets: CARRIER_TARGETS.map((map) => {
          const source = targets.find((target) => target.target_id === map.from) as Json
          const target = preset(map.to)
          return {
            ...scaleLatency(source, map.scale),
            target_id: target.id,
            target_name: target.name,
            provider: target.provider,
            packet_loss: 0
          }
        })
      }
    },
    records(data: Json[]): Json[] {
      const out: Json[] = []
      for (const map of CARRIER_TARGETS) {
        const series = data.filter((row) => row.target_id === map.from)
        series.forEach((row, index) => {
          const donor = series[(index + map.shift) % series.length]
          out.push({
            ...scaleLatency(donor, map.scale),
            timestamp: row.timestamp,
            target_id: map.to,
            packet_loss: 0,
            packet_received: donor.packet_sent
          })
        })
      }
      return out.sort((a, b) => String(a.timestamp).localeCompare(String(b.timestamp)))
    },
    serverTargets(): Json[] {
      return CARRIER_TARGETS.map((map) => preset(map.to))
    }
  }
}

type Unlock = 'blocked' | 'restricted' | 'unlocked'

/** Egress profile and unlock outcome per server; services not listed are unlocked. */
const IP_PROFILES: Record<
  string,
  {
    asn: string
    city: string
    country: string
    org: string
    risk: [string, number]
    type: string
    unlock: Record<string, Unlock>
  }
> = {
  '01': {
    country: 'US',
    city: 'San Jose',
    asn: 'AS906',
    org: 'DMIT Cloud Services',
    type: 'datacenter',
    risk: ['low', 18],
    unlock: {}
  },
  '02': {
    country: 'US',
    city: 'Los Angeles',
    asn: 'AS9929',
    org: 'China Unicom Global',
    type: 'isp',
    risk: ['low', 12],
    unlock: { tiktok: 'restricted' }
  },
  '03': {
    country: 'JP',
    city: 'Tokyo',
    asn: 'AS2914',
    org: 'NTT America',
    type: 'datacenter',
    risk: ['medium', 38],
    unlock: { netflix: 'restricted' }
  },
  '04': {
    country: 'SG',
    city: 'Singapore',
    asn: 'AS4809',
    org: 'China Telecom CN2',
    type: 'isp',
    risk: ['low', 9],
    unlock: {}
  },
  '05': {
    country: 'DE',
    city: 'Frankfurt',
    asn: 'AS24940',
    org: 'Hetzner Online',
    type: 'hosting',
    risk: ['high', 71],
    unlock: { netflix: 'blocked', disney_plus: 'blocked', chatgpt: 'restricted', hbo_max: 'blocked' }
  },
  '06': {
    country: 'NL',
    city: 'Amsterdam',
    asn: 'AS60781',
    org: 'LeaseWeb Netherlands',
    type: 'datacenter',
    risk: ['medium', 44],
    unlock: { prime_video: 'restricted' }
  },
  '07': {
    country: 'HK',
    city: 'Hong Kong',
    asn: 'AS4809',
    org: 'China Telecom CN2',
    type: 'isp',
    risk: ['low', 14],
    unlock: { chatgpt: 'blocked', gemini: 'blocked' }
  },
  '08': {
    country: 'AU',
    city: 'Sydney',
    asn: 'AS45102',
    org: 'Alibaba Cloud',
    type: 'datacenter',
    risk: ['medium', 33],
    unlock: { hbo_max: 'restricted' }
  },
  '09': {
    country: 'US',
    city: 'New York',
    asn: 'AS20473',
    org: 'The Constant Company',
    type: 'hosting',
    risk: ['medium', 52],
    unlock: { netflix: 'restricted', disney_plus: 'blocked' }
  },
  '10': {
    country: 'CA',
    city: 'Toronto',
    asn: 'AS16276',
    org: 'OVH SAS',
    type: 'hosting',
    risk: ['medium', 47],
    unlock: { netflix: 'restricted' }
  },
  '11': {
    country: 'GB',
    city: 'London',
    asn: 'AS14061',
    org: 'DigitalOcean',
    type: 'hosting',
    risk: ['high', 66],
    unlock: { netflix: 'blocked', prime_video: 'blocked', tiktok: 'restricted' }
  },
  '12': {
    country: 'IN',
    city: 'Bengaluru',
    asn: 'AS16509',
    org: 'Amazon.com',
    type: 'datacenter',
    risk: ['low', 21],
    unlock: { hbo_max: 'blocked' }
  }
}

function ipQualityOverview(servers: Json[], services: Json[]): Json[] {
  const checkedAt = new Date(Date.now() - 3 * 3600 * 1000).toISOString()
  return servers.map((server) => {
    const suffix = String(server.id).slice(-2)
    const profile = IP_PROFILES[suffix]
    if (!profile) {
      return { server_id: server.id, unlock_results: [], ip_quality: null }
    }
    return {
      server_id: server.id,
      unlock_results: services.map((service, index) => {
        const status = profile.unlock[String(service.key)] ?? 'unlocked'
        return {
          id: `capture-${suffix}-${index}`,
          server_id: server.id,
          service_id: service.id,
          status,
          region: status === 'blocked' ? null : profile.country,
          detail: status === 'restricted' && service.key === 'netflix' ? 'originals_only' : null,
          latency_ms: 180 + ((index * 37 + Number(suffix) * 13) % 240),
          checked_at: checkedAt
        }
      }),
      ip_quality: {
        ip: server.ipv4,
        ip_type: profile.type,
        country: profile.country,
        region: profile.city,
        city: profile.city,
        asn: profile.asn,
        as_org: profile.org,
        abuse_email: null,
        asn_abuser_score: null,
        risk_level: profile.risk[0],
        risk_score: profile.risk[1],
        is_proxy: false,
        is_vpn: profile.risk[0] === 'high',
        is_tor: false,
        is_abuser: profile.risk[0] === 'high',
        is_hosting: profile.type !== 'isp',
        is_mobile: false,
        checked_at: checkedAt
      }
    }
  })
}

/** A terminal grant with 24 minutes left on the focus server, as `serverbee-agent grant terminal --for 30m` leaves it. */
function withTemporaryGrant(server: Json): Json {
  if (server.id !== FOCUS) {
    return server
  }
  const now = Math.floor(Date.now() / 1000)
  const bit = 1
  return {
    ...server,
    // biome-ignore lint/suspicious/noBitwiseOperators: capabilities are a bitmask
    effective_capabilities: Number(server.effective_capabilities ?? server.capabilities ?? 0) | bit,
    temporary: [{ cap: 'terminal', granted_at: now - 6 * 60, expires_at: now + 24 * 60 }]
  }
}

function fleetOf(serverId: unknown) {
  return FLEET[String(serverId).slice(-2)]
}

/** The cycle usage of each server, as the share of its allowance the fleet table gives. */
function trafficOverview(data: Json[]): Json[] {
  return data.map((entry) => {
    const spec = fleetOf(entry.server_id)
    if (!spec) {
      return entry
    }
    const limit = Math.round(spec.limitTb * TB)
    const used = limit * spec.used
    return {
      ...entry,
      traffic_limit: limit,
      cycle_in: Math.round(used * 0.58),
      cycle_out: Math.round(used * 0.42),
      percent_used: spec.used * 100
    }
  })
}

/** How far a server's demo latency is from the latency the fleet table gives it. */
/** Ninety full days, all online but for a brief incident every few weeks, in place of the seed's two-day history. */
function uptimeDaily(data: Json[]): Json[] {
  const minutesToday = new Date().getUTCHours() * 60 + new Date().getUTCMinutes()
  return data.map((day, index) => {
    const total = index === data.length - 1 ? minutesToday : 1440
    const incident = index % 37 === 21
    return {
      ...day,
      total_minutes: total,
      online_minutes: incident ? total - 18 : total,
      downtime_incidents: incident ? 1 : 0
    }
  })
}

/**
 * History at one even spacing. The seed writes sparse history and the running server adds a record a minute, so an
 * untouched chart ends in a dense scribble.
 */
function evenRecords(data: Json[]): Json[] {
  const spacing = 12 * 60 * 1000
  let last = Number.NEGATIVE_INFINITY
  return data.filter((record) => {
    const time = Date.parse(String(record.time))
    if (Number.isNaN(time) || time - last < spacing) {
      return Number.isNaN(time)
    }
    last = time
    return true
  })
}

function latencyScale(entry: Json): number {
  const spec = fleetOf(entry.server_id)
  const targets = entry.targets as Json[]
  if (!(spec && targets.length > 0)) {
    return 1
  }
  const mean = targets.reduce((sum, target) => sum + Number(target.avg_latency ?? 0), 0) / targets.length
  return mean > 0 ? spec.latency / mean : 1
}

/** A full, mostly quiet probe history per server; two servers keep one loss blip so the strips are not uniform. */
function networkOverview(data: Json[]): Json[] {
  return data.map((entry, serverIndex) => {
    const spec = fleetOf(entry.server_id)
    if (!spec) {
      return entry
    }
    const scale = latencyScale(entry)
    const buckets = (entry.latency_sparkline as unknown[]).length
    const blip = serverIndex % 5 === 2 ? Math.floor(buckets * 0.6) : -1
    return {
      ...entry,
      anomaly_count: blip >= 0 ? 1 : 0,
      targets: (entry.targets as Json[]).map((target) => ({ ...scaleLatency(target, scale), packet_loss: 0 })),
      latency_sparkline: Array.from({ length: buckets }, (_, i) =>
        round1(spec.latency * (1 + 0.07 * Math.sin(i * 0.9 + serverIndex) + (i === blip ? 0.6 : 0)))
      ),
      loss_sparkline: Array.from({ length: buckets }, (_, i) => (i === blip ? 0.03 : 0))
    }
  })
}

/** Live probe results, brought onto the same latency scale as the overview and without the seed's random loss. */
function liveProbeMessage(text: string, scales: Map<string, number>): string {
  if (!text.includes('"network_probe_update"')) {
    return text
  }
  const message = JSON.parse(text) as Json
  const scale = scales.get(String(message.server_id)) ?? 1
  return JSON.stringify({
    ...message,
    results: (message.results as Json[]).map((result) => ({
      ...scaleLatency(result, scale),
      packet_loss: 0,
      packet_received: result.packet_sent
    }))
  })
}

async function fulfillJson(route: Route, transform: (data: never) => unknown) {
  const response = await route.fetch()
  const body = await response.json()
  await route.fulfill({ response, json: { ...body, data: transform(body.data as never) } })
}

async function installFixtures(context: BrowserContext, page: Page) {
  const presets = await api<ProbeTarget[]>(page, 'GET', '/network-probes/targets')
  const services = await api<Json[]>(page, 'GET', '/ip-quality/services')
  const carriers = carrierFixtures(presets)
  const probes = `**/api/servers/${FOCUS}/network-probes`

  await context.route(`${probes}/summary`, (route) => fulfillJson(route, carriers.summary))
  await context.route(`${probes}/records?*`, (route) => fulfillJson(route, carriers.records))
  await context.route(`${probes}/targets`, (route) => fulfillJson(route, carriers.serverTargets))
  await context.route(`${probes}/anomalies?*`, (route) => fulfillJson(route, () => []))
  await context.route('**/api/ip-quality/overview', async (route) => {
    const servers = await api<Json[]>(page, 'GET', '/servers')
    await fulfillJson(route, () => ipQualityOverview(servers, services))
  })
  await context.route('**/api/servers', (route) =>
    route.request().method() === 'GET'
      ? fulfillJson(route, (data: Json[]) => data.map(withTemporaryGrant))
      : route.continue()
  )
  await context.route(`**/api/servers/${FOCUS}`, (route) =>
    route.request().method() === 'GET' ? fulfillJson(route, withTemporaryGrant) : route.continue()
  )

  const overview = await api<Json[]>(page, 'GET', '/network-probes/overview')
  const scales = new Map(overview.map((entry) => [String(entry.server_id), latencyScale(entry)]))
  await context.route('**/api/network-probes/overview', (route) => fulfillJson(route, networkOverview))
  await context.route('**/api/traffic/overview', (route) => fulfillJson(route, trafficOverview))
  await context.route('**/api/servers/*/uptime-daily?*', (route) => fulfillJson(route, uptimeDaily))
  await context.route('**/api/servers/*/records?*', (route) => fulfillJson(route, evenRecords))
  await context.routeWebSocket('**/api/ws/servers', (socket) => {
    const upstream = socket.connectToServer()
    upstream.onMessage((message) => {
      socket.send(typeof message === 'string' ? liveProbeMessage(message, scales) : message)
    })
  })
}

/* ------------------------------------------------------------------- shots */

interface Rect {
  height: number
  width: number
  x: number
  y: number
}

interface Shot {
  /** The part of the page to keep. Omitted, the shot is the whole window. */
  clip?: (page: Page) => Promise<Rect>
  name: string
  path: () => string
  /** Runs before the screenshot, to reach the state the shot shows. */
  prepare?: (page: Page, lang: Lang) => Promise<void>
  /** Extra settling time after the page is idle, for charts that animate in. */
  settle?: number
  viewport: { height: number; width: number }
}

/** The smallest rectangle around the first visible match of each selector, grown by `pad` pixels. */
async function around(page: Page, selectors: string[], pad = 16): Promise<Rect> {
  const box = await page.evaluate((list) => {
    const rects = list.map((selector) => {
      const match = [...document.querySelectorAll(selector)].find(
        (element) => element.getBoundingClientRect().width > 0
      )
      if (!match) {
        throw new Error(`capture: nothing visible matches ${selector}`)
      }
      return match.getBoundingClientRect()
    })
    return {
      bottom: Math.max(...rects.map((rect) => rect.bottom)),
      left: Math.min(...rects.map((rect) => rect.left)),
      right: Math.max(...rects.map((rect) => rect.right)),
      top: Math.min(...rects.map((rect) => rect.top))
    }
  }, selectors)
  const x = Math.round(box.left - pad)
  const y = Math.round(box.top - pad)
  return { x, y, width: Math.round(box.right + pad) - x, height: Math.round(box.bottom + pad) - y }
}

/*
 * The hero shows the whole window. The feature sections show the part of a page they talk about, close to its real
 * size, so their viewports are sized to make that part fit without scrolling.
 */
const SHOT_LIST: Shot[] = [
  { name: 'fleet', path: () => '/servers?view=grid', viewport: { width: 1440, height: 900 }, settle: 4000 },
  {
    name: 'network',
    path: () => `/servers/${FOCUS}?tab=network&range=24h`,
    viewport: { width: 1280, height: 1200 },
    settle: 2500,
    prepare: async (page, lang) => {
      await page
        .getByText(lang === 'zh' ? '按运营商' : 'By Provider', { exact: true })
        .first()
        .click()
    },
    // The carrier columns and the latency chart under them.
    clip: (page) => around(page, ['[data-slot="tabs-content"] [data-slot="tabs"]', '[data-slot="tabs-content"] figure'])
  },
  {
    name: 'ip-quality',
    path: () => '/ip-quality',
    viewport: { width: 1440, height: 1500 },
    settle: 1500,
    // The last row of risk cards and the unlock matrix.
    clip: (page) =>
      around(
        page,
        ['div.space-y-2:has(> h2)', '[data-slot="card"]:nth-last-child(3)', '[data-slot="card"]:nth-last-child(1)'],
        8
      )
  },
  {
    name: 'capabilities',
    path: () => '/settings/capabilities',
    // Wide enough for the English table, whose columns do not shrink.
    viewport: { width: 1680, height: 900 },
    settle: 1500,
    clip: (page) => around(page, ['[data-slot="table-container"]'], 0)
  },
  {
    name: 'dashboard',
    path: () => '/',
    viewport: { width: 1440, height: 1200 },
    settle: 5000,
    // The content pane, from its header down to the end of the widget grid.
    clip: async (page) => {
      const pane = await around(page, ['main'], -1)
      const grid = await around(page, ['.dashboard-grid'])
      return { ...pane, height: grid.y + grid.height - pane.y }
    }
  }
]

async function openContext(
  lang: Lang,
  theme: Theme,
  viewport: Shot['viewport'],
  storageState: string,
  dashboard: string
) {
  const browser = await chromium.launch()
  const context = await browser.newContext({
    viewport,
    deviceScaleFactor: 2,
    colorScheme: theme,
    locale: lang === 'zh' ? 'zh-CN' : 'en-US',
    timezoneId: 'UTC',
    reducedMotion: 'no-preference',
    storageState
  })
  await context.addInitScript(
    ([l, t, d]) => {
      localStorage.setItem('i18nextLng', l)
      localStorage.setItem('theme', t)
      localStorage.setItem('serverbee.dashboard.selected', d)
    },
    [lang, theme, dashboard] as const
  )
  const page = await context.newPage()
  await page.goto(`${BASE}/login`)
  await installFixtures(context, page)
  return { browser, context, page }
}

async function open(page: Page, shot: Shot, lang: Lang) {
  await page.goto(BASE + shot.path(), { waitUntil: 'networkidle' })
  await shot.prepare?.(page, lang)
  await page.waitForTimeout(shot.settle ?? 1000)
  // Keyboard focus rings and hover cards are interaction state, not part of the product's resting look.
  await page.mouse.move(0, 0)
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())
}

async function captureShots(storageState: string, dashboard: string) {
  mkdirSync(SHOTS, { recursive: true })
  for (const shot of SHOT_LIST.filter((s) => !only || s.name === only)) {
    for (const lang of LANGS) {
      for (const theme of THEMES) {
        const { browser, page } = await openContext(lang, theme, shot.viewport, storageState, dashboard)
        try {
          await open(page, shot, lang)
          const png = await page.screenshot({ type: 'png', clip: await shot.clip?.(page) })
          const file = join(SHOTS, `${shot.name}-${lang}-${theme}.webp`)
          await sharp(png).webp({ lossless: true, effort: 6 }).toFile(file)
          console.log(`shot  ${shot.name}-${lang}-${theme}`)
        } finally {
          await browser.close()
        }
      }
    }
  }
}

async function main() {
  const browser = await chromium.launch()
  const context = await browser.newContext()
  const page = await context.newPage()
  await page.goto(`${BASE}/login`)
  await api(page, 'POST', '/auth/login', { username: 'admin', password: 'admin123' })
  const dashboard = await stage(page)
  const storageState = join(mkdtempSync(join(tmpdir(), 'serverbee-capture-')), 'state.json')
  await context.storageState({ path: storageState })
  await browser.close()

  await captureShots(storageState, dashboard)
}

await main()
