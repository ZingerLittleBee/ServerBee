import { getLatencyTextClass } from './network-latency-constants'

export interface NetworkProbeTarget {
  created_at: string | null
  id: string
  location: string
  name: string
  probe_type: string
  provider: string
  source: string | null
  source_name: string | null
  target: string
  updated_at: string | null
}

export interface NetworkProbeSetting {
  default_target_ids: string[]
  interval: number
  packet_count: number
}

export interface NetworkProbeRecord {
  avg_latency: number | null
  id: number
  max_latency: number | null
  min_latency: number | null
  packet_loss: number
  packet_received: number
  packet_sent: number
  server_id: string
  target_id: string
  timestamp: string
}

export interface NetworkTargetSummary {
  availability: number
  avg_latency: number | null
  max_latency: number | null
  min_latency: number | null
  packet_loss: number
  provider: string
  target_id: string
  target_name: string
}

export interface NetworkServerSummary {
  anomaly_count: number
  last_probe_at: string | null
  latency_sparkline: (number | null)[]
  loss_sparkline: (number | null)[]
  online: boolean
  server_id: string
  server_name: string
  targets: NetworkTargetSummary[]
}

export interface NetworkProbeAnomaly {
  anomaly_type: string
  target_id: string
  target_name: string
  timestamp: string
  value: number
}

export function formatLatency(ms: number | null | undefined): string {
  if (ms == null) {
    return 'N/A'
  }
  return `${ms.toFixed(1)} ms`
}

export function formatPacketLoss(loss: number): string {
  return `${(loss * 100).toFixed(1)}%`
}

export interface NetworkProbeResultData {
  avg_latency: number | null
  max_latency: number | null
  min_latency: number | null
  packet_loss: number
  packet_received: number
  packet_sent: number
  target_id: string
  timestamp: string
}

export type RecordedProtocol = 'icmp' | 'udp' | 'tcp' | 'legacy'
export type TraceProtocol = 'icmp' | 'udp' | 'tcp'

// Rust serializes Option::None with skip_serializing_if = "Option::is_none",
// so old-agent JSON OMITS the new keys entirely. In JS that means
// `total_sent === undefined`, not null. All discriminator checks MUST use
// loose `value != null` to catch both undefined and null.
export interface TracerouteHop {
  asn: string | null
  avg_ms?: number | null
  best_ms?: number | null
  hop: number
  hostname: string | null
  // Legacy fields (filled only by old shell-based agents)
  ip?: string | null
  // New fields (filled by trippy-core agent); absent from old-agent payloads
  ips?: string[]
  jitter_ms?: number | null
  loss_pct?: number | null
  rtt1?: number | null
  rtt2?: number | null
  rtt3?: number | null
  stddev_ms?: number | null
  total_recv?: number | null
  total_sent?: number | null
  worst_ms?: number | null
}

export interface TracerouteResult {
  completed: boolean
  completed_at: number | null
  error: string | null
  hops: TracerouteHop[]
  /** 'legacy' = run by a pre-trippy agent; actual probe protocol unknown */
  protocol: RecordedProtocol
  request_id: string
  round: number
  started_at: number
  target: string
  total_rounds: number
}

export interface TracerouteRecordSummary {
  completed_at: number | null
  has_error: boolean
  hop_count: number
  protocol: RecordedProtocol
  request_id: string
  started_at: number
  target: string
}

export interface TracerouteResponse {
  request_id: string
}

export function isNewSchemaHop(hop: TracerouteHop): boolean {
  return hop.total_sent != null
}

export const PROVIDER_LABELS: Record<string, string> = {
  ct: 'China Telecom',
  cu: 'China Unicom',
  cm: 'China Mobile',
  international: 'International'
}

export function getProviderLabel(provider: string): string {
  return PROVIDER_LABELS[provider] ?? provider
}

export function latencyColorClass(ms: number | null, options?: { failed?: boolean }): string {
  return getLatencyTextClass({ latencyMs: ms, failed: options?.failed })
}

export function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio >= 0.5) {
    return 'text-destructive font-semibold'
  }
  if (lossRatio >= 0.1) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return ''
}
