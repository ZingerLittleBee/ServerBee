/* Network quality panel: HK · CN2 GIA against six carrier targets. Chips and carrier rows show the
   window average, like the product's avg_latency; the latency chart sits below both views. */

import type { CSSProperties } from 'react'

import type { IconName } from '../icon'
import { type CarrierId, type CountryCode, carrierIds, mockServer, probeTargets } from '../mock-data'
import { LANDING_VERSION, type LandingLang, landingCopy } from '../translations'
import { webSpecs } from './fleet'
import { type ChartLine, f1, mean, pts, series } from './shared'

/** `all`: one chip per target. `prov`: one column per carrier. */
export type NetView = 'all' | 'prov'

/** Top of the latency chart's scale. */
export const LATENCY_MAX_MS = 150

/** Labelled grid lines on the latency chart: the top of the scale and every third below it. */
const GRID_STEPS = 3

/** Ticks per view while the visitor has not picked one. */
const TICKS_PER_VIEW = 6

/** The visitor's choice sticks; until then the view alternates every six ticks. */
export function resolveNetView(tick: number, chosen: NetView | null): NetView {
  if (chosen) {
    return chosen
  }
  return Math.floor(tick / TICKS_PER_VIEW) % 2 === 0 ? 'all' : 'prov'
}

interface NetTab {
  /** `np-tab`, or `np-tab on` for the Network tab. */
  className: string
  icon: IconName
  id: string
  label: string
}

interface NetRange {
  /** `np-range`, or `np-range on` for Realtime. */
  className: string
  id: string
  label: string
}

interface NetChip {
  dotStyle: CSSProperties
  id: string
  lat: string
  /** Loss value only; the template adds the "Packet Loss" label so CSS can hide it when space runs out. */
  loss: string
  name: string
}

interface NetProviderRow {
  dotStyle: CSSProperties
  id: string
  lat: string
  name: string
}

interface NetProvider {
  avg: string
  id: CarrierId
  loss: string
  name: string
  rows: NetProviderRow[]
}

interface NetGridLine {
  /** "150 ms", "100 ms", "50 ms". */
  label: string
  /** Position from the top of the plot, e.g. `33.333%`. */
  top: string
}

interface NetHead {
  /** OS, vCPU count, memory and agent version, shown dot-separated under the name. */
  facts: string[]
  flag: CountryCode
  name: string
}

interface NetworkModel {
  /** `np-seg-btn`, plus `on` for the active view. */
  allClassName: string
  chips: NetChip[]
  grid: NetGridLine[]
  head: NetHead
  isAll: boolean
  isProv: boolean
  lines: ChartLine[]
  provClassName: string
  provs: NetProvider[]
  ranges: NetRange[]
  tabs: NetTab[]
}

const tabItems: { id: string; icon: IconName }[] = [
  { id: 'metrics', icon: 'cpu' },
  { id: 'network', icon: 'activity' },
  { id: 'traffic', icon: 'chart-no-axes-column' },
  { id: 'security', icon: 'shield' },
  { id: 'ip-quality', icon: 'shield-check' }
]

const rangeIds = ['realtime', '1h', '6h', '24h', '7d', '30d'] as const

interface ProviderTotals {
  avgs: number[]
  id: CarrierId
  name: string
  rows: NetProviderRow[]
}

export function buildNetwork(lang: LandingLang, tick: number, view: NetView): NetworkModel {
  const p = landingCopy[lang].p
  const lines: ChartLine[] = []
  const chips: NetChip[] = []
  const provs: ProviderTotals[] = carrierIds.map((id, index) => ({
    id,
    name: p.provNames[index],
    rows: [],
    avgs: []
  }))
  for (const [index, target] of probeTargets.entries()) {
    const values = series(index + 3, target.base, target.amp, 48, tick, target.spikeEvery, target.spike)
    const avg = mean(values)
    const dotStyle = { background: target.color }
    const name = target.name[lang]
    lines.push({ id: target.id, style: { color: target.color }, pts: pts(values, LATENCY_MAX_MS) })
    chips.push({ id: target.id, name, dotStyle, lat: `${f1(avg)} ms`, loss: '0.0%' })
    provs[target.isp].rows.push({ id: target.id, name, dotStyle, lat: `${f1(avg)} ms` })
    provs[target.isp].avgs.push(avg)
  }
  const isAll = view === 'all'
  const server = mockServer('hk')
  const grid: NetGridLine[] = []
  for (let step = 0; step < GRID_STEPS; step += 1) {
    grid.push({
      label: `${LATENCY_MAX_MS - (step * LATENCY_MAX_MS) / GRID_STEPS} ms`,
      top: `${Number(((step * 100) / GRID_STEPS).toFixed(3))}%`
    })
  }
  return {
    head: { flag: server.flag, name: server.name, facts: [...webSpecs(server), `Agent v${LANDING_VERSION}`] },
    grid,
    tabs: tabItems.map((item, index) => ({
      ...item,
      label: p.tabs[index],
      className: item.id === 'network' ? 'np-tab on' : 'np-tab'
    })),
    ranges: rangeIds.map((id, index) => ({
      id,
      label: p.ranges[index],
      className: id === 'realtime' ? 'np-range on' : 'np-range'
    })),
    isAll,
    isProv: !isAll,
    allClassName: isAll ? 'np-seg-btn on' : 'np-seg-btn',
    provClassName: isAll ? 'np-seg-btn' : 'np-seg-btn on',
    chips,
    provs: provs.map(({ avgs, ...provider }) => ({ ...provider, avg: f1(mean(avgs)), loss: '0.0%' })),
    lines
  }
}
