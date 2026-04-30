/**
 * Convenience re-exports from the auto-generated OpenAPI types.
 * Import from here instead of defining hand-written interfaces.
 *
 * Regenerate: pnpm run generate:api-types
 */
import type { components } from './api-types'

type S = components['schemas']

// Auth
export type LoginRequest = S['LoginRequest']
export type LoginResponse = S['LoginResponse']
export type MeResponse = S['MeResponse']

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
export type RecoveryCandidateResponse = S['RecoveryCandidateResponse']
export type RecoveryJobResponse = S['RecoveryJobResponse']
export type RecoveryJobStage = S['RecoveryJobStage']
export type RecoveryJobStatus = S['RecoveryJobStatus']
export type StartRecoveryRequest = S['StartRecoveryRequest']

// Server groups
export type ServerGroup = S['ServerGroup']
export type CreateGroupRequest = S['CreateGroupRequest']
export type UpdateGroupRequest = S['UpdateGroupRequest']

// Alert rules
export type AlertRule = S['AlertRule']
export type AlertRuleItem = S['AlertRuleItem']
export type CreateAlertRule = S['CreateAlertRule']
export type UpdateAlertRule = S['UpdateAlertRule']

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

// Status page (public)
export type StatusPageResponse = S['StatusPageResponse']
export type StatusServer = S['StatusServer']
export type StatusMetrics = S['StatusMetrics']
export type StatusGroup = S['StatusGroup']

// Agent
export type RegisterResponse = S['RegisterResponse']
export type UpgradeRequest = S['UpgradeRequest']
export type AutoDiscoveryKeyResponse = S['AutoDiscoveryKeyResponse']

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

export type ThemeVars = Record<string, string>

export type ThemeResolved =
  | {
      id: string
      kind: 'preset'
    }
  | {
      id: number
      kind: 'custom'
      name: string
      updated_at: string
      vars_dark: ThemeVars
      vars_light: ThemeVars
    }

// Public status page (slug-based)
export interface PublicStatusPageData {
  active_incidents: Array<{
    created_at: string
    id: string
    severity: string
    status: string
    title: string
    updates: Array<{
      created_at: string
      id: string
      message: string
      status: string
    }>
  }>
  theme: ThemeResolved
  page: {
    custom_css: string | null
    description: string | null
    show_values: boolean
    title: string
    uptime_red_threshold: number
    uptime_yellow_threshold: number
  }
  planned_maintenances: Array<{
    description: string | null
    end_at: string
    id: string
    start_at: string
    title: string
  }>
  recent_incidents: Array<{
    created_at: string
    id: string
    resolved_at: string | null
    severity: string
    status: string
    title: string
    updates: Array<{
      created_at: string
      id: string
      message: string
      status: string
    }>
  }>
  servers: Array<{
    group_name: string | null
    in_maintenance: boolean
    online: boolean
    server_id: string
    server_name: string
    uptime_daily: UptimeDailyEntry[]
    uptime_percent: number | null
  }>
}

// Admin status page management
export interface StatusPageItem {
  created_at: string
  description: string | null
  enabled: boolean
  id: string
  server_ids: string[]
  slug: string
  title: string
  updated_at: string
  uptime_red_threshold: number
  uptime_yellow_threshold: number
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
