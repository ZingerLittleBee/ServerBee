export const CAP_TERMINAL = 1
export const CAP_EXEC = 2
export const CAP_UPGRADE = 4
export const CAP_PING_ICMP = 8
export const CAP_PING_TCP = 16
export const CAP_PING_HTTP = 32
export const CAP_DEFAULT = 56

export const CAPABILITIES = [
  { bit: CAP_TERMINAL, key: 'terminal', label: 'Web Terminal', risk: 'high' as const },
  { bit: CAP_EXEC, key: 'exec', label: 'Remote Exec', risk: 'high' as const },
  { bit: CAP_UPGRADE, key: 'upgrade', label: 'Auto Upgrade', risk: 'high' as const },
  { bit: CAP_PING_ICMP, key: 'ping_icmp', label: 'ICMP Ping', risk: 'low' as const },
  { bit: CAP_PING_TCP, key: 'ping_tcp', label: 'TCP Probe', risk: 'low' as const },
  { bit: CAP_PING_HTTP, key: 'ping_http', label: 'HTTP Probe', risk: 'low' as const }
] as const

export function hasCap(capabilities: number, bit: number): boolean {
  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
  return (capabilities & bit) !== 0
}
