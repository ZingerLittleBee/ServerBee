// Bit definitions and metadata are generated from the Rust
// ALL_CAPABILITIES table (bun run generate:capabilities); the helpers
// below are hand-written web logic on top of them.
import { CAP_DEFAULT, CAPABILITIES, type CapabilityRisk } from './capability-bits.generated'

// biome-ignore lint/performance/noBarrelFile: capabilities.ts stays the single public surface — the generated bit definitions re-export through it so import sites don't need to know about codegen
export {
  CAP_DEFAULT,
  CAP_DOCKER,
  CAP_EXEC,
  CAP_FILE,
  CAP_FIREWALL_BLOCK,
  CAP_IP_QUALITY,
  CAP_PING_HTTP,
  CAP_PING_ICMP,
  CAP_PING_TCP,
  CAP_SECURITY_EVENTS,
  CAP_TERMINAL,
  CAP_UPGRADE,
  CAPABILITIES,
  type CapabilityRisk
} from './capability-bits.generated'

// i18n keys (servers namespace) for each risk tier's badge label. Single source
// so the settings table and the capabilities dialog stay in sync.
export const RISK_LABEL_KEY: Record<CapabilityRisk, 'cap_high_risk' | 'cap_medium_risk' | 'cap_low_risk'> = {
  high: 'cap_high_risk',
  medium: 'cap_medium_risk',
  low: 'cap_low_risk'
}

// Tailwind text-color class per risk tier for the compact risk labels.
export const RISK_TEXT_CLASS: Record<CapabilityRisk, string> = {
  high: 'text-red-500',
  medium: 'text-amber-500',
  low: 'text-muted-foreground'
}

export function hasCap(capabilities: number, bit: number): boolean {
  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
  return (capabilities & bit) !== 0
}

// Capabilities are agent-owned: the server mirrors what the agent reports, so the
// effective, agent-local and mirrored `capabilities` values are all the same set.
// This resolves whether a capability bit is enabled, preferring the live runtime
// values and falling back to the persisted mirror (then CAP_DEFAULT) when an agent
// has never connected.
export function getEffectiveCapabilityEnabled(
  effectiveCapabilities: number | null | undefined,
  configuredCapabilities: number | null | undefined,
  bit: number
): boolean {
  if (effectiveCapabilities != null) {
    return hasCap(effectiveCapabilities, bit)
  }
  return hasCap(configuredCapabilities ?? CAP_DEFAULT, bit)
}

export type CapabilityState = 'off' | 'enabled' | 'temporary'

export interface TemporaryGrantView {
  cap: string
  expires_at: number
  granted_at: number
}

interface CapabilityHost {
  capabilities?: number | null
  effective_capabilities?: number | null
  temporary?: TemporaryGrantView[] | null
}

const CAP_BY_BIT = new Map<number, string>(CAPABILITIES.map((c) => [c.bit, c.key]))

// Returns the active grant for a capability bit, if any (expiry checked client-side).
export function temporaryGrantFor(host: CapabilityHost, bit: number): TemporaryGrantView | undefined {
  const key = CAP_BY_BIT.get(bit)
  if (!(key && host.temporary)) {
    return undefined
  }
  const nowSecs = Math.floor(Date.now() / 1000)
  return host.temporary.find((g) => g.cap === key && g.expires_at > nowSecs)
}

export function classifyCapability(host: CapabilityHost, bit: number): CapabilityState {
  const enabled = getEffectiveCapabilityEnabled(host.effective_capabilities, host.capabilities, bit)
  if (!enabled) {
    return 'off'
  }
  return temporaryGrantFor(host, bit) ? 'temporary' : 'enabled'
}
