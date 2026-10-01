/* iOS app mocks: the tab bar shared by every phone, the server list with its push banner, and the
   TYO · BGP IP-quality screen. */

import type { CSSProperties } from 'react'

import type { IconName } from '../icon'
import {
  type AccessCategory,
  mockServer,
  PUSH_TITLE,
  phoneAccess,
  type ServerId,
  type ServiceId,
  TYO_RISK_SCORE
} from '../mock-data'
import { type LandingLang, landingCopy } from '../translations'
import { buildServerCards, iosSpecs } from './fleet'
import { clamp, f1, type SegItem } from './shared'

interface IosTab {
  /** Alerts carries a "1" badge. */
  badge: boolean
  /** `ios-tab`, or `ios-tab on` for Servers. */
  className: string
  icon: IconName
  id: 'servers' | 'alerts' | 'insights' | 'settings'
  label: string
}

interface PhoneBar {
  id: 'cpu' | 'mem' | 'disk'
  label: string
  /** Fill width and color of the `<i>` inside `.ios-bt`. */
  style: CSSProperties
  val: string
}

interface PhoneRow {
  /** Empty while offline. */
  bars: PhoneBar[]
  /** CPU at or above the warning level: the row shows "CPU high" and counts on the Alerts tile. */
  cpuHigh: boolean
  details: string
  /** `ios-dot on|warn|off`. */
  dotClassName: string
  id: ServerId
  name: string
  online: boolean
  trail: string
  /** `ios-trail`, plus `warn` (CPU high) or `off` (offline). */
  trailClassName: string
}

interface PhoneAccessRow {
  /** `ios-st ok|blk`. */
  className: string
  id: ServiceId
  name: string
  /** Detected region (ChatGPT only), otherwise empty. */
  region: string
  st: string
}

interface PhoneAccessGroupView {
  cat: string
  id: AccessCategory
  rows: PhoneAccessRow[]
}

interface PhoneServerHead {
  name: string
  os: string
  /** "2 vCPU · 4 GB". */
  specs: string
}

export interface IosModel {
  /** The Alerts tile: rows showing "CPU high". */
  alertCount: number
  chipOnlineTyo: string
  /** Fleet download rate for the "Traffic ↓" tile, in MB/s. */
  fleetDownV: string
  /** Sets `--v` (the risk score) for `.ios-ring`. */
  ipRingStyle: CSSProperties
  /** The risk ring's score, the same one the web IP card shows. */
  ipRisk: string
  /** Title and chips of the IP screen: TYO · BGP. */
  ipServer: PhoneServerHead
  /** The Online tile: rows online, out of `serverCount`. */
  onlineCount: number
  phoneIp: PhoneAccessGroupView[]
  phoneRows: PhoneRow[]
  pushBody: string
  pushTitle: string
  /** The IP screen's segments before its selected "IP" segment. */
  segIp: SegItem[]
  serverCount: number
}

const tabItems: { id: IosTab['id']; icon: IconName }[] = [
  { id: 'servers', icon: 'server' },
  { id: 'alerts', icon: 'bell' },
  { id: 'insights', icon: 'chart-column' },
  { id: 'settings', icon: 'settings' }
]

/** The tab bar of every phone mock (Servers selected, one alert). */
export function buildIosTabs(lang: LandingLang): IosTab[] {
  const tabs = landingCopy[lang].p.ios.tabs
  return tabItems.map((item, index) => ({
    ...item,
    label: tabs[index],
    className: item.id === 'servers' ? 'ios-tab on' : 'ios-tab',
    badge: item.id === 'alerts'
  }))
}

const ipSegIds = ['overview', 'metrics', 'network', 'traffic'] as const

/** Servers at or above this CPU usage show the warning dot and "CPU high". */
const CPU_WARN = 85

export function buildIos(lang: LandingLang, tick: number): IosModel {
  const p = landingCopy[lang].p
  const ios = p.ios
  const cards = buildServerCards(lang, tick)
  const categoryNames: Record<AccessCategory, string> = { ai: 'AI', stream: p.cats[0], social: p.cats[2] }
  const bar = (id: PhoneBar['id'], label: string, value: number, color: string): PhoneBar => ({
    id,
    label,
    val: `${value}%`,
    style: { width: `${clamp(value, 2, 100)}%`, background: color }
  })
  let fleetDown = 0
  for (const card of cards) {
    fleetDown += card.online ? card.net[0].raw : 0
  }
  const phoneRows = cards.map((card): PhoneRow => {
    const high = card.online && card.cpu >= CPU_WARN
    let dotClassName = 'ios-dot off'
    let trail = ios.offlineAgo
    let trailClassName = 'ios-trail off'
    if (high) {
      dotClassName = 'ios-dot warn'
      trail = ios.cpuHigh
      trailClassName = 'ios-trail warn'
    } else if (card.online) {
      dotClassName = 'ios-dot on'
      trail = `↓${f1(card.net[0].raw)} ↑${f1(card.net[1].raw)} MB/s`
      trailClassName = 'ios-trail'
    }
    return {
      id: card.id,
      name: card.name,
      cpuHigh: high,
      dotClassName,
      trail,
      trailClassName,
      details: `${card.ip} · ${card.os}`,
      online: card.online,
      bars: card.online
        ? [
            bar('cpu', 'CPU', card.cpu, 'var(--ios-cpu)'),
            bar('mem', ios.memShort, card.mem, 'var(--ios-mem)'),
            bar('disk', ios.diskShort, card.disk, 'var(--ios-disk)')
          ]
        : []
    }
  })
  const ipServer = mockServer('tyo')

  return {
    segIp: ipSegIds.map((id, index) => ({ id, label: ios.segIp[index] })),
    chipOnlineTyo: ios.chipOnlineTyo,
    ipServer: { name: ipServer.name, os: ipServer.os, specs: iosSpecs(ipServer) },
    ipRisk: String(TYO_RISK_SCORE),
    ipRingStyle: { '--v': String(TYO_RISK_SCORE) },
    phoneRows,
    onlineCount: phoneRows.filter((row) => row.online).length,
    serverCount: phoneRows.length,
    alertCount: phoneRows.filter((row) => row.cpuHigh).length,
    phoneIp: phoneAccess.map((group) => ({
      id: group.cat,
      cat: categoryNames[group.cat],
      rows: group.rows.map((row) => ({
        id: row.id,
        name: row.name,
        st: p.st[row.st],
        className: `ios-st ${row.st}`,
        region: row.region ?? ''
      }))
    })),
    pushTitle: PUSH_TITLE,
    pushBody: `Alert rule ‘${ios.pushRule}’ triggered (cpu >= 90)`,
    fleetDownV: f1(fleetDown)
  }
}
