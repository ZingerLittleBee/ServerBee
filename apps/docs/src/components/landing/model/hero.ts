/* Hero: the install box, the web window (servers page), the small phone (HK · CN2 GIA network) and the live
   preview toggle. */

import type { CSSProperties } from 'react'

import type { IconName } from '../icon'
import { type CarrierId, carrierIds, probeTargets } from '../mock-data'
import { type InstallTab, installCommands, type LandingLang, landingCopy } from '../translations'
import { buildServerCards, type ServerCard } from './fleet'
import { type ChartLine, type CommandSegment, mean, pts, type SegItem, segs, series } from './shared'

export interface NavItem {
  /** `pw-nav`, or `pw-nav on` for the active page (Servers). */
  className: string
  icon: IconName
  id: string
  label: string
}

interface PhoneTargetRow {
  /** Series color for the `<i>` dot. */
  dotStyle: CSSProperties
  id: string
  lat: string
  loss: string
  name: string
}

interface PhoneTargetGroup {
  id: CarrierId
  name: string
  rows: PhoneTargetRow[]
}

export interface HeroModel {
  /** The first four server cards. */
  cards: ServerCard[]
  phoneNet: { lines: ChartLine[]; groups: PhoneTargetGroup[] }
  /** The phone's detail segments before "More", with Network selected. */
  segNet: SegItem[]
  segNetMore: string
  sideNav: NavItem[]
}

const sideNavItems: { id: string; icon: IconName }[] = [
  { id: 'dashboard', icon: 'layout-dashboard' },
  { id: 'servers', icon: 'server' },
  { id: 'network', icon: 'activity' },
  { id: 'traffic', icon: 'chart-column' },
  { id: 'security', icon: 'shield' }
]

const detailSegIds = ['overview', 'metrics', 'network', 'traffic'] as const

/** 30 samples per target, 18 buckets ahead of the network section so the two charts differ. */
function phoneNet(lang: LandingLang, tick: number): HeroModel['phoneNet'] {
  const p = landingCopy[lang].p
  const lines: ChartLine[] = []
  const groups: PhoneTargetGroup[] = carrierIds.map((id, index) => ({ id, name: p.provNames[index], rows: [] }))
  for (const [index, target] of probeTargets.entries()) {
    const values = series(index + 3, target.base, target.amp, 30, tick + 18, target.spikeEvery, target.spike)
    lines.push({ id: target.id, style: { color: target.color }, pts: pts(values, 100) })
    groups[target.isp].rows.push({
      id: target.id,
      name: target.name[lang],
      dotStyle: { background: target.color },
      lat: `${Math.round(mean(values))} ms`,
      loss: '0%'
    })
  }
  return { lines, groups }
}

export function buildHero(lang: LandingLang, tick: number): HeroModel {
  const p = landingCopy[lang].p
  return {
    sideNav: sideNavItems.map((item, index) => ({
      ...item,
      label: p.nav[index],
      className: item.id === 'servers' ? 'pw-nav on' : 'pw-nav'
    })),
    cards: buildServerCards(lang, tick).slice(0, 4),
    segNet: detailSegIds.map((id, index) => ({
      id,
      label: p.ios.segNet[index],
      className: id === 'network' ? 'on' : undefined
    })),
    segNetMore: p.ios.segNet[4],
    phoneNet: phoneNet(lang, tick)
  }
}

interface InstallTabItem {
  /** `ins-tab`, or `ins-tab on` for the selected tab. */
  className: string
  id: InstallTab
  label: string
  selected: boolean
}

/** One tab's body. All three are rendered in one grid cell; only the selected one is visible. */
interface InstallPane {
  /** `ins-pane` (with `ins-rail` for Railway), plus `on` for the selected tab's body. */
  className: string
  id: InstallTab
  /** Railway shows a deploy link instead of a command. */
  isRailway: boolean
  /** Empty for Railway. */
  segments: CommandSegment[]
  selected: boolean
}

/** The copy button's feedback: none yet, a copied command, or a failed write (the command is selected instead). */
export type CopyState = 'idle' | 'copied' | 'failed'

interface CopyFeedback {
  icon: IconName
  label: string
  /** Announced by the install box's status region; empty while idle. */
  status: string
}

interface InstallModel {
  /** The exact command the copy button writes, empty for Railway. */
  command: string
  copyIcon: IconName
  copyLabel: string
  copyStatus: string
  /** Railway shows a deploy link instead of a command. */
  isRailway: boolean
  panes: InstallPane[]
  tabs: InstallTabItem[]
}

const installTabs: InstallTab[] = ['docker', 'binary', 'railway']

function paneClassName(id: InstallTab, selected: boolean): string {
  const base = id === 'railway' ? 'ins-pane ins-rail' : 'ins-pane'
  return selected ? `${base} on` : base
}

export function buildInstall(lang: LandingLang, tab: InstallTab, copyState: CopyState): InstallModel {
  const hero = landingCopy[lang].hero
  const feedback: Record<CopyState, CopyFeedback> = {
    idle: { icon: 'copy', label: hero.copy, status: '' },
    copied: { icon: 'check', label: hero.copied, status: hero.copiedStatus },
    failed: { icon: 'copy', label: hero.copyFailed, status: hero.copyFailedStatus }
  }
  return {
    tabs: installTabs.map((id) => ({
      id,
      label: hero.tabs[id],
      className: id === tab ? 'ins-tab on' : 'ins-tab',
      selected: id === tab
    })),
    panes: installTabs.map((id) => ({
      id,
      className: paneClassName(id, id === tab),
      isRailway: id === 'railway',
      segments: id === 'railway' ? [] : segs(installCommands[id]),
      selected: id === tab
    })),
    isRailway: tab === 'railway',
    command: tab === 'railway' ? '' : installCommands[tab],
    copyLabel: feedback[copyState].label,
    copyIcon: feedback[copyState].icon,
    copyStatus: feedback[copyState].status
  }
}

/** The hero's pause and resume button for the live mocks; its visible label is its accessible name. */
export function buildLiveToggle(lang: LandingLang, live: boolean): { icon: IconName; label: string } {
  const hero = landingCopy[lang].hero
  return live ? { icon: 'pause', label: hero.livePause } : { icon: 'play', label: hero.liveResume }
}
