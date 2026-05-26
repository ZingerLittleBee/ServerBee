/**
 * Convenience re-exports from the auto-generated OpenAPI types.
 * Import from here instead of defining hand-written interfaces.
 *
 * Regenerate: pnpm run generate:api-types
 */
import type { components } from './api-types'

export type { IpQualitySnapshotData, ServerIpQualityData } from './ip-quality-types'

type S = components['schemas']

// Auth
export type LoginRequest = S['LoginRequest']
export type LoginResponse = S['LoginResponse']
export type MeResponse = S['MeResponse']
export type OnboardingRequest = S['OnboardingRequest']

// Users
export type UserResponse = S['UserResponse']
export type CreateUserInput = S['CreateUserInput']
export type UpdateUserInput = S['UpdateUserInput']
export type ChangePasswordRequest = S['ChangePasswordRequest']

// Servers
export type ServerResponse = S['ServerResponse']
export type ServerMetricRecord = S['ServerRecord']
export type UpdateServerInput = S['UpdateServerInput']
export type BatchDeleteRequest = S['BatchDeleteRequest']
export type BatchDeleteResponse = S['BatchDeleteResponse']
export type CreateServerRequest = S['CreateServerRequest']
export type CreateServerResponse = S['CreateServerResponse']
export type EnrollmentIssueResponse = S['EnrollmentIssueResponse']
export type OutstandingEnrollmentSummary = S['OutstandingEnrollmentSummary']
export type RecoverRequest = S['RecoverRequest']
export type RecoverResponse = S['RecoverResponse']
export type RegenerateCodeRequest = S['RegenerateCodeRequest']
export type RegenerateCodeResponse = S['RegenerateCodeResponse']

// Cost
export type CostOverviewResponse = S['CostOverviewResponse']
export type CurrencyCostSummary = S['CurrencyCostSummary']
export type ServerCostOverview = S['ServerCostOverview']
export type ServerCostInsights = S['ServerCostInsights']
export type ResourceValue = S['ResourceValue']
export type ValueScore = S['ValueScore']
export type CostInvalidReason = S['CostInvalidReason']
export type ValueGrade = S['ValueGrade']
export type ValueReason = S['ValueReason']
export type ValueConfidence = S['ValueConfidence']

// Server groups
export type ServerGroup = S['ServerGroup']
export type CreateGroupRequest = S['CreateGroupRequest']
export type UpdateGroupRequest = S['UpdateGroupRequest']

// Alert rules
export type AlertRule = S['AlertRule']
export type AlertRuleItem = S['AlertRuleItem']
export type CreateAlertRule = S['CreateAlertRule']
export type UpdateAlertRule = S['UpdateAlertRule']
export type SecurityRuleParams = S['SecurityRuleParams']

// Security events
export type SecurityEventDto = S['SecurityEventDto']
export type SecurityEventList = S['SecurityEventList']
export type StatsBucket = S['StatsBucket']

export interface AlertStateResponse {
  count: number
  first_triggered_at: string
  last_notified_at: string
  resolved: boolean
  resolved_at: string | null
  server_id: string
  server_name: string
}

// Notifications
export type Notification = S['Notification']
export type NotificationGroup = S['NotificationGroup']
export type CreateNotification = S['CreateNotification']
export type UpdateNotification = S['UpdateNotification']
export type CreateNotificationGroup = S['CreateNotificationGroup']
export type UpdateNotificationGroup = S['UpdateNotificationGroup']

// Ping
export type PingTask = S['PingTask']
export type PingRecord = S['PingRecord']
export type CreatePingTask = S['CreatePingTask']
export type UpdatePingTask = S['UpdatePingTask']

// Tasks
export type TaskResponse = S['TaskResponse']
export type TaskResult = S['TaskResult']
export type CreateTaskRequest = S['CreateTaskRequest']

// GPU
export type GpuRecord = S['GpuRecord']

// Audit
export type AuditLogEntry = S['AuditLogEntry']
export type AuditListResponse = S['AuditListResponse']
export type AuditOptionsResponse = S['AuditOptionsResponse']
export type AuditUserOption = S['AuditUserOption']

// API keys
export type ApiKeyResponse = S['ApiKeyResponse']
export type CreateApiKeyRequest = S['CreateApiKeyRequest']

// OAuth
export type OAuthAccount = S['OAuthAccount']
export type OAuthProvidersResponse = S['OAuthProvidersResponse']

// 2FA / TOTP
export type TotpStatusResponse = S['TotpStatusResponse']
export type TotpSetupResponse = S['TotpSetupResponse']
export type TotpVerifyRequest = S['TotpVerifyRequest']
export type TotpDisableRequest = S['TotpDisableRequest']

// System settings
export type SystemSettings = S['SystemSettings']

// Agent
export type RegisterResponse = S['RegisterResponse']
export type UpgradeRequest = S['UpgradeRequest']
export type EnrollmentSummary = S['EnrollmentSummary']
export type RotateTokenResponse = S['RotateTokenResponse']

// Traffic (manually typed until OpenAPI types are regenerated)
export interface TrafficResponse {
  bytes_in: number
  bytes_out: number
  bytes_total: number
  cycle_end: string
  cycle_start: string
  daily: Array<{ bytes_in: number; bytes_out: number; date: string }>
  hourly: Array<{ bytes_in: number; bytes_out: number; hour: string }>
  prediction: TrafficPrediction | null
  traffic_limit: number | null
  traffic_limit_type: string | null
  usage_percent: number | null
}

export interface TrafficPrediction {
  estimated_percent: number
  estimated_total: number
  will_exceed: boolean
}

// Uptime daily entry (from /api/servers/{id}/uptime-daily)
export interface UptimeDailyEntry {
  date: string
  downtime_incidents: number
  online_minutes: number
  total_minutes: number
}

export type ThemeResolved = S['ThemeResolved']

// ---------------------------------------------------------------------------
// Public status page DTOs (singleton, mirror of `crates/server/src/service/public_status.rs`).
// Field names match the Rust JSON shape exactly (snake_case); no IP-level
// identifiers or free-form leak fields are present here by design.
// ---------------------------------------------------------------------------

export interface PublicStatusConfig {
  default_layout: 'list' | 'grid'
  description: string | null
  enabled: boolean
  show_incidents: boolean
  show_ip_quality: boolean
  show_maintenance: boolean
  show_network: boolean
  show_server_detail: boolean
  title: string
  uptime_red_threshold: number
  uptime_yellow_threshold: number
}

export interface PublicMetricsSummary {
  cpu: number
  disk_total: number
  disk_used: number
  load_1: number
  load_5: number
  load_15: number
  mem_total: number
  mem_used: number
  net_in_speed: number
  net_out_speed: number
  uptime: number
}

export interface PublicServerSummary {
  country_code: string | null
  group_name: string | null
  id: string
  in_maintenance: boolean
  metrics: PublicMetricsSummary | null
  name: string
  online: boolean
  os: string | null
  public_remark: string | null
  region: string | null
  uptime_daily: UptimeDailyEntry[]
  uptime_percent: number | null
}

export type PublicServerDetail = PublicServerSummary & {
  cpu_name: string | null
  cpu_cores: number | null
  cpu_arch: string | null
  kernel_version: string | null
  agent_version: string | null
  mem_total: number | null
  disk_total: number | null
  process_count: number | null
  tcp_conn: number | null
  udp_conn: number | null
}

export interface PublicIpQualitySnapshot {
  checked_at: string
  country: string | null
  ip_type: string
  risk_level: string
  risk_score: number | null
}

export interface PublicUnlockResult {
  checked_at: string
  latency_ms: number | null
  region: string | null
  service_id: string
  status: string
}

export interface PublicIpQualityEntry {
  /** Absent until the agent has reported at least once. */
  ip_quality: PublicIpQualitySnapshot | null
  server_id: string
  unlock_results: PublicUnlockResult[]
}

export interface PublicIpQualityServiceMeta {
  category: string
  id: string
  is_builtin: boolean
  key: string
  name: string
  popularity: number
}

export interface PublicIpQualityOverview {
  entries: PublicIpQualityEntry[]
  services: PublicIpQualityServiceMeta[]
}

export interface PublicIncidentUpdate {
  created_at: string
  id: string
  message: string
  status: string
}

export interface PublicIncident {
  created_at: string
  id: string
  resolved_at: string | null
  severity: string
  status: string
  title: string
  updates: PublicIncidentUpdate[]
}

export interface PublicMaintenance {
  description: string | null
  end_at: string
  id: string
  start_at: string
  title: string
}

export interface PublicIncidentsResponse {
  active: PublicIncident[]
  recent: PublicIncident[]
}

// Network DTOs mirror `service::network_probe::{TargetSummary, ServerSummary,
// ServerOverview, NetworkProbeAnomaly}`. The auth'd TS layer in
// `lib/network-types.ts` conflates `ServerSummary` and `ServerOverview` into a
// single `NetworkServerSummary` shape; here we mirror the Rust shapes
// faithfully because the public surface is the contract source of truth.

export interface PublicNetworkTargetSummary {
  availability: number
  avg_latency: number | null
  max_latency: number | null
  min_latency: number | null
  packet_loss: number
  provider: string
  target_id: string
  target_name: string
}

export interface PublicNetworkServerOverview {
  anomaly_count: number
  last_probe_at: string | null
  latency_sparkline: (number | null)[]
  loss_sparkline: (number | null)[]
  online: boolean
  server_id: string
  server_name: string
  targets: PublicNetworkTargetSummary[]
}

export interface PublicNetworkServerSummary {
  anomaly_count: number
  last_probe_at: string | null
  online: boolean
  server_id: string
  server_name: string
  targets: PublicNetworkTargetSummary[]
}

export interface PublicNetworkProbeAnomaly {
  anomaly_type: string
  target_id: string
  target_name: string
  timestamp: string
  value: number
}

export interface PublicNetworkOverview {
  servers: PublicNetworkServerOverview[]
}

export interface PublicNetworkServerDetail {
  anomalies: PublicNetworkProbeAnomaly[]
  summary: PublicNetworkServerSummary
}

export interface PublicMetricsPoint {
  cpu: number
  disk_used: number
  gpu_usage: number | null
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  process_count: number
  tcp_conn: number
  temperature: number | null
  time: string
  udp_conn: number
}

export interface PublicMetricsRangeQuery {
  from: string
  /** "auto" | "raw" | "hourly". Defaults to "auto" server-side. */
  interval?: string
  to: string
}

// ---------------------------------------------------------------------------
// Admin status-page (singleton, mirror of
// `crates/server/src/service/status_page.rs`). After R1 there is exactly one
// row; GET returns the full model, PUT accepts a partial-update payload.
// ---------------------------------------------------------------------------

export interface StatusPageItem {
  created_at: string
  default_layout: 'list' | 'grid'
  description: string | null
  enabled: boolean
  id: string
  server_ids: string[]
  show_incidents: boolean
  show_ip_quality: boolean
  show_maintenance: boolean
  show_network: boolean
  show_server_detail: boolean
  title: string
  updated_at: string
  uptime_red_threshold: number
  uptime_yellow_threshold: number
}

/** Partial PATCH-style payload for `PUT /api/admin/status-page`. */
export interface UpdateStatusPageRequest {
  default_layout?: 'list' | 'grid'
  description?: string | null
  enabled?: boolean
  server_ids?: string[]
  show_incidents?: boolean
  show_ip_quality?: boolean
  show_maintenance?: boolean
  show_network?: boolean
  show_server_detail?: boolean
  title?: string
  uptime_red_threshold?: number
  uptime_yellow_threshold?: number
}

export interface IncidentItem {
  created_at: string
  id: string
  resolved_at: string | null
  server_ids: string[]
  severity: string
  status: string
  title: string
  updated_at: string
  updates: Array<{
    created_at: string
    id: string
    message: string
    status: string
  }>
}

export interface MaintenanceItem {
  active: boolean
  created_at: string
  description: string | null
  end_at: string
  id: string
  server_ids: string[]
  start_at: string
  title: string
  updated_at: string
}

// Errors
export type ErrorBody = S['ErrorBody']
export type ErrorDetail = S['ErrorDetail']
