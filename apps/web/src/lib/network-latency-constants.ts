const LATENCY_HEALTHY_THRESHOLD_MS = 300
const NETWORK_FAILURE_PACKET_LOSS_RATIO = 1

const LATENCY_UNKNOWN_TEXT_CLASS = 'text-muted-foreground'
const LATENCY_HEALTHY_TEXT_CLASS = 'text-status-healthy-text'
const LATENCY_WARNING_TEXT_CLASS = 'text-status-warning-text'
const LATENCY_FAILED_TEXT_CLASS = 'text-status-danger-text'

// "No data" squares read as an empty slot in the grid, so they take the border
// tone rather than the near-invisible muted surface.
export const LATENCY_UNKNOWN_BAR_COLOR = 'var(--color-border)'
const LATENCY_HEALTHY_BAR_COLOR = 'var(--status-healthy)'
const LATENCY_WARNING_BAR_COLOR = 'var(--status-warning)'
const LATENCY_FAILED_BAR_COLOR = 'var(--status-danger)'

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

export const LOSS_WARNING_THRESHOLD_RATIO = 0.01
export const LOSS_SEVERE_THRESHOLD_RATIO = 0.05

export type CombinedSeverity = 'unknown' | 'healthy' | 'warning' | 'severe' | 'failed'

interface CombinedSeverityInput {
  latencyMs: number | null | undefined
  lossRatio: number | null | undefined
}

export function getCombinedSeverity({ latencyMs, lossRatio }: CombinedSeverityInput): CombinedSeverity {
  if (lossRatio != null && lossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO) {
    return 'failed'
  }
  if (lossRatio != null && lossRatio >= LOSS_SEVERE_THRESHOLD_RATIO) {
    return 'severe'
  }
  if (latencyMs == null && lossRatio == null) {
    return 'unknown'
  }
  const latencyWarn = latencyMs != null && latencyMs >= LATENCY_HEALTHY_THRESHOLD_MS
  const lossWarn = lossRatio != null && lossRatio >= LOSS_WARNING_THRESHOLD_RATIO
  if (latencyWarn || lossWarn) {
    return 'warning'
  }
  return 'healthy'
}

export function getCombinedBarColor(input: CombinedSeverityInput): string {
  switch (getCombinedSeverity(input)) {
    case 'healthy':
      return LATENCY_HEALTHY_BAR_COLOR
    case 'warning':
      return LATENCY_WARNING_BAR_COLOR
    case 'severe':
    case 'failed':
      return LATENCY_FAILED_BAR_COLOR
    default:
      return LATENCY_UNKNOWN_BAR_COLOR
  }
}

// Text tone for an end-to-end packet loss ratio, shared by the server card, its
// per-target tooltip and the network quality widget.
export function getLossTextClass(lossRatio: number | null | undefined): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < LOSS_WARNING_THRESHOLD_RATIO) {
    return 'text-status-healthy-text'
  }
  if (lossRatio < LOSS_SEVERE_THRESHOLD_RATIO) {
    return 'text-status-warning-text'
  }
  return 'text-status-danger-text'
}

export function getLossDotBgClass(lossRatio: number | null | undefined): string {
  if (lossRatio == null) {
    return 'bg-muted-foreground'
  }
  if (lossRatio < LOSS_WARNING_THRESHOLD_RATIO) {
    return 'bg-status-healthy'
  }
  if (lossRatio < LOSS_SEVERE_THRESHOLD_RATIO) {
    return 'bg-status-warning'
  }
  return 'bg-status-danger'
}

export function getLatencySquareColor({ latencyMs, lossRatio }: CombinedSeverityInput): string {
  if (lossRatio != null && lossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO) {
    return LATENCY_FAILED_BAR_COLOR
  }
  if (latencyMs == null) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (latencyMs < LATENCY_HEALTHY_THRESHOLD_MS) {
    return LATENCY_HEALTHY_BAR_COLOR
  }
  return LATENCY_WARNING_BAR_COLOR
}

export function getLossSquareColor(lossRatio: number | null | undefined): string {
  if (lossRatio == null) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (lossRatio < LOSS_WARNING_THRESHOLD_RATIO) {
    return LATENCY_HEALTHY_BAR_COLOR
  }
  if (lossRatio < LOSS_SEVERE_THRESHOLD_RATIO) {
    return LATENCY_WARNING_BAR_COLOR
  }
  return LATENCY_FAILED_BAR_COLOR
}
