/* Security: the capabilities panel of HK · CN2 GIA and the terminal grant command. */

import { mockServer } from '../mock-data'
import { type CapabilityId, type LandingLang, landingCopy } from '../translations'
import { type CommandSegment, segs } from './shared'

interface HighRiskCapability {
  id: CapabilityId
  name: string
  /** "已启用" / "已关闭", "Enabled" / "Disabled". */
  st: string
  /** `cap-st`, or `cap-st on` while enabled. */
  stClassName: string
  /** Shows the "Temporary" badge: the terminal runs on a time-limited grant. */
  tmp: boolean
}

interface LowRiskCapability {
  id: CapabilityId
  name: string
  /** Risk label shown in the chip's `<small>`. */
  risk: string
}

interface SecurityModel {
  capHigh: HighRiskCapability[]
  capLow: LowRiskCapability[]
  /** Segments of `security.grant` for the terminal. */
  grantSegments: CommandSegment[]
  /** The server whose capabilities the panel lists. */
  server: string
}

/** Only the terminal is on, through a temporary grant made on the host. */
const highRisk: { id: CapabilityId; enabled: boolean; temporary: boolean }[] = [
  { id: 'terminal', enabled: true, temporary: true },
  { id: 'exec', enabled: false, temporary: false },
  { id: 'file', enabled: false, temporary: false },
  { id: 'docker', enabled: false, temporary: false }
]

const lowRisk: { id: CapabilityId; medium: boolean }[] = [
  { id: 'icmp', medium: false },
  { id: 'tcp', medium: false },
  { id: 'http', medium: false },
  { id: 'securityEvents', medium: false },
  { id: 'firewall', medium: true },
  { id: 'ipQuality', medium: true }
]

export function buildSecurity(lang: LandingLang): SecurityModel {
  const copy = landingCopy[lang]
  const p = copy.p
  return {
    capHigh: highRisk.map(({ id, enabled, temporary }) => ({
      id,
      name: p.capNames[id],
      tmp: temporary,
      st: enabled ? p.enabled : p.disabled,
      stClassName: enabled ? 'cap-st on' : 'cap-st'
    })),
    capLow: lowRisk.map(({ id, medium }) => ({
      id,
      name: p.capNames[id],
      risk: medium ? p.riskMed : p.riskLow
    })),
    grantSegments: segs(copy.security.grant),
    server: mockServer('hk').name
  }
}
