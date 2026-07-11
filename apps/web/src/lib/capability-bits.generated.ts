// AUTO-GENERATED from crates/common/src/constants.rs (ALL_CAPABILITIES).
// Regenerate with `bun run generate:capabilities` — do not edit by hand.

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

// The OR of every default_enabled bit: upgrade + ping_icmp + ping_tcp + ping_http + security_events + firewall_block + ip_quality.
export const CAP_DEFAULT = 1852

export const CAPABILITIES = [
  { bit: CAP_TERMINAL, key: 'terminal', labelKey: 'cap_terminal', risk: 'high' },
  { bit: CAP_EXEC, key: 'exec', labelKey: 'cap_exec', risk: 'high' },
  { bit: CAP_UPGRADE, key: 'upgrade', labelKey: 'cap_upgrade', risk: 'low' },
  { bit: CAP_PING_ICMP, key: 'ping_icmp', labelKey: 'cap_ping_icmp', risk: 'low' },
  { bit: CAP_PING_TCP, key: 'ping_tcp', labelKey: 'cap_ping_tcp', risk: 'low' },
  { bit: CAP_PING_HTTP, key: 'ping_http', labelKey: 'cap_ping_http', risk: 'low' },
  { bit: CAP_FILE, key: 'file', labelKey: 'cap_file', risk: 'high' },
  { bit: CAP_DOCKER, key: 'docker', labelKey: 'cap_docker', risk: 'high' },
  { bit: CAP_SECURITY_EVENTS, key: 'security_events', labelKey: 'cap_security_events', risk: 'low' },
  { bit: CAP_FIREWALL_BLOCK, key: 'firewall_block', labelKey: 'cap_firewall_block', risk: 'medium' },
  { bit: CAP_IP_QUALITY, key: 'ip_quality', labelKey: 'cap_ip_quality', risk: 'medium' }
] as const

export type CapabilityRisk = (typeof CAPABILITIES)[number]['risk']
