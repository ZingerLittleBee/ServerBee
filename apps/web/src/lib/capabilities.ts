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
    risk: 'high' as const
  },
  {
    bit: CAP_IP_QUALITY,
    key: 'ip_quality',
    labelKey: 'cap_ip_quality' as const,
    // 'medium' mirrors the Rust risk_level, but the UI renders this in the low/non-destructive
    // risk group — capability-toggle risk grouping is binary (high vs. not-high).
    risk: 'medium' as const
  }
] as const

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
