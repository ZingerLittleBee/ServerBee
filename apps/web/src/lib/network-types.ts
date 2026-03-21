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

export interface TracerouteHop {
  asn: string | null
  hop: number
  hostname: string | null
  ip: string | null
  rtt1: number | null
  rtt2: number | null
  rtt3: number | null
}

export interface TracerouteResult {
  completed: boolean
  error: string | null
  hops: TracerouteHop[]
  target: string
}

export interface TracerouteResponse {
  request_id: string
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

export function latencyColorClass(ms: number | null): string {
  if (ms == null) {
    return 'text-muted-foreground'
  }
  if (ms < 50) {
    return 'text-green-600 dark:text-green-400'
  }
  if (ms < 100) {
    return 'text-yellow-600 dark:text-yellow-400'
  }
  return 'text-red-600 dark:text-red-400'
}
