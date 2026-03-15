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
