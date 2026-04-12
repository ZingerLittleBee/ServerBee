export const LATENCY_HEALTHY_THRESHOLD_MS = 300
export const NETWORK_FAILURE_PACKET_LOSS_RATIO = 1

export const LATENCY_UNKNOWN_TEXT_CLASS = 'text-muted-foreground'
export const LATENCY_HEALTHY_TEXT_CLASS = 'text-emerald-600 dark:text-emerald-400'
export const LATENCY_WARNING_TEXT_CLASS = 'text-amber-600 dark:text-amber-400'
export const LATENCY_FAILED_TEXT_CLASS = 'text-red-600 dark:text-red-400'

export const LATENCY_UNKNOWN_BAR_COLOR = 'var(--color-muted)'
export const LATENCY_HEALTHY_BAR_COLOR = '#10b981'
export const LATENCY_WARNING_BAR_COLOR = '#f59e0b'
export const LATENCY_FAILED_BAR_COLOR = '#ef4444'

export type LatencyStatus = 'unknown' | 'healthy' | 'warning' | 'failed'

interface LatencyStatusInput {
  failed?: boolean
  latencyMs: number | null | undefined
}

export function isLatencyFailure(packetLossRatio: number | null | undefined): boolean {
  return packetLossRatio != null && packetLossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO
}

export function getLatencyStatus({ latencyMs, failed = false }: LatencyStatusInput): LatencyStatus {
  if (failed) {
    return 'failed'
  }
  if (latencyMs == null) {
    return 'unknown'
  }
  if (latencyMs < LATENCY_HEALTHY_THRESHOLD_MS) {
    return 'healthy'
  }
  return 'warning'
}

export function getLatencyTextClass(input: LatencyStatusInput): string {
  switch (getLatencyStatus(input)) {
    case 'healthy':
      return LATENCY_HEALTHY_TEXT_CLASS
    case 'warning':
      return LATENCY_WARNING_TEXT_CLASS
    case 'failed':
      return LATENCY_FAILED_TEXT_CLASS
    case 'unknown':
      return LATENCY_UNKNOWN_TEXT_CLASS
    default:
      return LATENCY_UNKNOWN_TEXT_CLASS
  }
}

export function getLatencyBarColor(input: LatencyStatusInput): string {
  switch (getLatencyStatus(input)) {
    case 'healthy':
      return LATENCY_HEALTHY_BAR_COLOR
    case 'warning':
      return LATENCY_WARNING_BAR_COLOR
    case 'failed':
      return LATENCY_FAILED_BAR_COLOR
    case 'unknown':
      return LATENCY_UNKNOWN_BAR_COLOR
    default:
      return LATENCY_UNKNOWN_BAR_COLOR
  }
}
