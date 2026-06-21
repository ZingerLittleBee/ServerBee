export const CAP_TERMINAL = 1
export const CAP_EXEC = 2
export const CAP_UPGRADE = 4
export const CAP_PING_ICMP = 8
export const CAP_PING_TCP = 16
export const CAP_PING_HTTP = 32
export const CAP_FILE = 64
export const CAP_DOCKER = 128
export const CAP_SECURITY_EVENTS = 256
export const CAP_FIREWALL_BLOCK = 512
export const CAP_IP_QUALITY = 1024

// Mirrors CAP_DEFAULT in crates/common/src/constants.rs (1852):
// upgrade + ICMP/TCP/HTTP ping + security events + firewall blocklist + IP quality.
export const CAP_DEFAULT = 1852

export const CAPABILITIES = [
  { bit: CAP_TERMINAL, key: 'terminal', labelKey: 'cap_terminal' as const, risk: 'high' as const },
  { bit: CAP_EXEC, key: 'exec', labelKey: 'cap_exec' as const, risk: 'high' as const },
  { bit: CAP_UPGRADE, key: 'upgrade', labelKey: 'cap_upgrade' as const, risk: 'low' as const },
  { bit: CAP_PING_ICMP, key: 'ping_icmp', labelKey: 'cap_ping_icmp' as const, risk: 'low' as const },
  { bit: CAP_PING_TCP, key: 'ping_tcp', labelKey: 'cap_ping_tcp' as const, risk: 'low' as const },
  { bit: CAP_PING_HTTP, key: 'ping_http', labelKey: 'cap_ping_http' as const, risk: 'low' as const },
  { bit: CAP_FILE, key: 'file', labelKey: 'cap_file' as const, risk: 'high' as const },
  { bit: CAP_DOCKER, key: 'docker', labelKey: 'cap_docker' as const, risk: 'high' as const },
  {
    bit: CAP_SECURITY_EVENTS,
    key: 'security_events',
    labelKey: 'cap_security_events' as const,
    risk: 'low' as const
  },
  {
    bit: CAP_FIREWALL_BLOCK,
    key: 'firewall_block',
    labelKey: 'cap_firewall_block' as const,
    // Medium, not high: the agent can only add/remove IPs in its own dedicated
    // nft blocklist set (see crates/agent/src/firewall) — it can't exec code,
    // read files, or flush the host firewall, and guardrails reject self-lockout
    // ranges. Mirrors the Rust risk_level in crates/common/src/constants.rs.
    risk: 'medium' as const
  },
  {
    bit: CAP_IP_QUALITY,
    key: 'ip_quality',
    labelKey: 'cap_ip_quality' as const,
    // Mirrors the Rust risk_level in crates/common/src/constants.rs.
    risk: 'medium' as const
  }
] as const

export type CapabilityRisk = (typeof CAPABILITIES)[number]['risk']

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
