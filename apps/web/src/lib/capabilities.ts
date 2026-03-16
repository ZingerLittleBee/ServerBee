export const CAP_TERMINAL = 1
export const CAP_EXEC = 2
export const CAP_UPGRADE = 4
export const CAP_PING_ICMP = 8
export const CAP_PING_TCP = 16
export const CAP_PING_HTTP = 32
export const CAP_FILE = 64
export const CAP_DEFAULT = 56

export const CAPABILITIES = [
  { bit: CAP_TERMINAL, key: 'terminal', labelKey: 'cap_terminal' as const, risk: 'high' as const },
  { bit: CAP_EXEC, key: 'exec', labelKey: 'cap_exec' as const, risk: 'high' as const },
  { bit: CAP_UPGRADE, key: 'upgrade', labelKey: 'cap_upgrade' as const, risk: 'high' as const },
  { bit: CAP_PING_ICMP, key: 'ping_icmp', labelKey: 'cap_ping_icmp' as const, risk: 'low' as const },
  { bit: CAP_PING_TCP, key: 'ping_tcp', labelKey: 'cap_ping_tcp' as const, risk: 'low' as const },
  { bit: CAP_PING_HTTP, key: 'ping_http', labelKey: 'cap_ping_http' as const, risk: 'low' as const },
  { bit: CAP_FILE, key: 'file', labelKey: 'cap_file' as const, risk: 'high' as const }
] as const

export function hasCap(capabilities: number, bit: number): boolean {
  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
  return (capabilities & bit) !== 0
}
