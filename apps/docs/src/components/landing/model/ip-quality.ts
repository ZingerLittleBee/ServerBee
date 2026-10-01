/* IP quality panel: three egress IP cards and the unlock matrix. */

import { ipCards, type ServerId, type ServiceId, type UnlockCell, unlockRows, unlockServices } from '../mock-data'
import { type LandingLang, landingCopy } from '../translations'

interface IpCard {
  flags: { id: string; label: string }[]
  id: ServerId
  ip: string
  loc: string
  name: string
  /** "风险 38 · 中", "Risk 38 · medium". */
  risk: string
  /** `ipc-risk low|mid|high`. */
  riskClassName: string
  type: string
}

interface MatrixService {
  /** `mx-th`, plus `end` on the last column of a category. */
  className: string
  id: ServiceId
  name: string
}

interface MatrixCell {
  /** Cell div: `mx-td`, plus `end` on the last column of a category. */
  cellClassName: string
  /** Badge span: `mx-b ok|lim|blk|fail`, or `mx-dash` without data. */
  className: string
  id: ServiceId
  /** Status label, or a dash placeholder without data. */
  label: string
}

interface MatrixRow {
  cells: MatrixCell[]
  id: ServerId
  name: string
}

interface IpQualityModel {
  cards: IpCard[]
  catSocial: string
  /** Category headers: streaming spans 5 columns, AI 2 (the literal "AI"), social 2. */
  catStreaming: string
  services: MatrixService[]
  unlock: MatrixRow[]
}

const badgeClassNames: Record<UnlockCell, string> = {
  ok: 'mx-b ok',
  lim: 'mx-b lim',
  blk: 'mx-b blk',
  fail: 'mx-b fail',
  none: 'mx-dash'
}

/** Columns that close a category (HBO Max ends streaming, Google Gemini ends AI) carry `end`. */
const categoryEnds = new Set<ServiceId>(['hbo', 'gemini'])

export function buildIpQuality(lang: LandingLang): IpQualityModel {
  const p = landingCopy[lang].p
  return {
    cards: ipCards.map((card) => ({
      id: card.id,
      name: card.name,
      ip: card.ip,
      type: card.type[lang],
      risk: `${p.risk} ${card.score} · ${card.level[lang]}`,
      riskClassName: `ipc-risk ${card.tone}`,
      flags: card.flags.map((flag) => ({ id: flag.id, label: flag.label[lang] })),
      loc: card.loc
    })),
    catStreaming: p.cats[0],
    catSocial: p.cats[2],
    services: unlockServices.map((service) => ({
      ...service,
      className: categoryEnds.has(service.id) ? 'mx-th end' : 'mx-th'
    })),
    unlock: unlockRows.map((row) => ({
      id: row.id,
      name: row.name,
      cells: unlockServices.map((service, index) => {
        const state = row.cells[index]
        return {
          id: service.id,
          label: state === 'none' ? '—' : p.st[state],
          className: badgeClassNames[state],
          cellClassName: categoryEnds.has(service.id) ? 'mx-td end' : 'mx-td'
        }
      })
    }))
  }
}
