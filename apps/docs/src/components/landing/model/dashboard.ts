/* Dashboard in edit mode. Values and formats follow the product widgets (stat-number, gauge, top-n,
   multi-line), and the fleet numbers come from the same server cards as the hero. */

import type { CSSProperties } from 'react'

import type { IconName } from '../icon'
import { cpuCompare, type ServerId, TRAFFIC_SCALE_GB, trafficDays, uptimeRows } from '../mock-data'
import { type LandingLang, landingCopy } from '../translations'
import { buildServerCards, type ServerCard } from './fleet'
import { f1, noise, pts, series, usageColor } from './shared'

interface DashStat {
  /** `dw dw-stat g-s<n> tone-<n>`. */
  className: string
  icon: IconName
  id: 'servers' | 'cpu' | 'mem' | 'bw'
  label: string
  sub: string
  val: string
}

interface DashSeries {
  /** Legend dot color. */
  dotStyle: CSSProperties
  id: ServerId
  name: string
  pts: string
  /** Line color for the polyline. */
  style: CSSProperties
}

export interface DashGauge {
  icon: IconName
  label: string
  /** Server name under the gauge. */
  server: string
  /** Sets `--v` and `--c` for the `.gauge` element. */
  style: CSSProperties
  val: string
}

interface DashTopRow {
  id: ServerId
  name: string
  /** Bar width (memory usage). */
  style: CSSProperties
}

interface DashUptimeRow {
  /** 90 days; `className` is `db-up up|deg|down`. */
  cells: { id: string; className: string }[]
  id: ServerId
  name: string
  pct: string
}

interface DashTrafficBar {
  /** Day label ("09-24"), unique. */
  id: string
  inStyle: CSSProperties
  label: string
  outStyle: CSSProperties
}

interface DashboardModel {
  bars: DashTrafficBar[]
  cmp: DashSeries[]
  gaugeA: DashGauge
  gaugeB: DashGauge
  /** Markdown runbook lines. */
  mdLines: { id: string; text: string }[]
  stats: DashStat[]
  top: DashTopRow[]
  uptime: DashUptimeRow[]
}

const UPTIME_DAYS = 90
/** Noise stream for the static uptime rows; it gives the design's day counts (100.00%, 99.93%, 99.65%). */
const UPTIME_STREAM = 55

function cardById(cards: ServerCard[], id: ServerId): ServerCard {
  const card = cards.find((candidate) => candidate.id === id)
  if (!card) {
    throw new Error(`Unknown mock server: ${id}`)
  }
  return card
}

function gauge(label: string, server: string, value: number, icon: IconName): DashGauge {
  return { label, server, val: f1(value), icon, style: { '--v': f1(value), '--c': usageColor(value) } }
}

function uptimeRow(id: ServerId, name: string, seed: number, bad: number): DashUptimeRow {
  const cells: DashUptimeRow['cells'] = []
  let impaired = 0
  for (let day = 0; day < UPTIME_DAYS; day += 1) {
    const h = noise(UPTIME_STREAM, seed, day)
    let state = 'up'
    if (h > 1 - bad * 0.25) {
      state = 'down'
    } else if (h > 1 - bad) {
      state = 'deg'
    }
    if (state !== 'up') {
      impaired += 1
    }
    cells.push({ id: String(day), className: `db-up ${state}` })
  }
  return { id, name, cells, pct: `${(100 - impaired * 0.07).toFixed(2)}%` }
}

function percentOfScale(gb: number): string {
  return `${((gb / TRAFFIC_SCALE_GB) * 100).toFixed(2)}%`
}

export function buildDashboard(lang: LandingLang, tick: number): DashboardModel {
  const w = landingCopy[lang].p.w
  const cards = buildServerCards(lang, tick)
  const online = cards.filter((card) => card.online)
  const offline = cards.length - online.length
  const average = (pick: (card: ServerCard) => number) =>
    online.reduce((sum, card) => sum + pick(card), 0) / online.length
  const bandwidth = online.reduce((sum, card) => sum + card.net[0].raw + card.net[1].raw, 0)
  const fra = cardById(cards, 'fra')
  const tyo = cardById(cards, 'tyo')

  const stats: Omit<DashStat, 'className'>[] = [
    {
      id: 'servers',
      label: w.servers,
      val: `${online.length} / ${cards.length}`,
      sub: `${offline}${w.offlineSuffix}`,
      icon: 'server'
    },
    { id: 'cpu', label: w.avgCpu, val: `${f1(average((card) => card.cpuRaw))}%`, sub: w.serversSub, icon: 'cpu' },
    {
      id: 'mem',
      label: w.avgMem,
      val: `${f1(average((card) => card.memRaw))}%`,
      sub: w.serversSub,
      icon: 'memory-stick'
    },
    { id: 'bw', label: w.bw, val: `${f1(bandwidth)} MB`, sub: '/s', icon: 'wifi' }
  ]

  return {
    stats: stats.map((stat, index) => ({ ...stat, className: `dw dw-stat g-s${index + 1} tone-${index + 1}` })),
    cmp: cpuCompare.map((line, index) => {
      const values = series(index + 40, line.base, line.amp, 44, tick, line.spikeEvery, line.spike)
      return {
        id: line.id,
        name: line.name,
        style: { color: line.color },
        dotStyle: { background: line.color },
        pts: pts(values, 50)
      }
    }),
    gaugeA: gauge(w.gauge, fra.name, fra.cpuRaw, 'cpu'),
    gaugeB: gauge(w.gauge2, tyo.name, tyo.memRaw, 'memory-stick'),
    top: online
      .slice()
      .sort((a, b) => b.memRaw - a.memRaw)
      .map((card) => ({ id: card.id, name: card.name, style: { width: `${f1(card.memRaw)}%` } })),
    uptime: uptimeRows.map((row) => uptimeRow(row.id, row.name, row.seed, row.bad)),
    bars: trafficDays.map((day) => ({
      id: day.id,
      label: day.id,
      inStyle: { height: percentOfScale(day.inbound) },
      outStyle: { height: percentOfScale(day.outbound) }
    })),
    mdLines: w.mdLines.map((text) => ({ id: text, text }))
  }
}
