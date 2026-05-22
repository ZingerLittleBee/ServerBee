// ---------------------------------------------------------------------------
// Service catalog
// ---------------------------------------------------------------------------

export interface UnlockService {
  category: string
  created_at: string
  detector: string | null
  enabled: boolean
  id: string
  is_builtin: boolean
  key: string
  name: string
  popularity: number
  /** JSON string: custom request config */
  request: string | null
  /** JSON string: custom match rules */
  rules: string | null
  updated_at: string
}

// ---------------------------------------------------------------------------
// Protocol DTOs (mirror crates/common/src/protocol.rs)
// All structs lack #[serde(rename_all)] so field names are snake_case.
// UnlockStatus enum uses #[serde(rename_all = "snake_case")] so values
// are "unlocked", "restricted", "blocked", "failed", "unsupported".
// ---------------------------------------------------------------------------

export type UnlockStatus = 'unlocked' | 'restricted' | 'blocked' | 'failed' | 'unsupported'

export interface UnlockRequest {
  headers: [string, string][]
  method: string
  timeout_ms: number
  url: string
}

/** UnlockMatch uses #[serde(tag = "kind", rename_all = "snake_case")] */
export type UnlockMatch =
  | { kind: 'status_equals'; code: number }
  | { kind: 'status_in_range'; min: number; max: number }
  | { kind: 'body_regex'; pattern: string }
  | { kind: 'redirect_matches'; pattern: string }

export interface UnlockRule {
  /** Rust field is `match_` with #[serde(rename = "match")] */
  match: UnlockMatch
  result: UnlockStatus
}

export interface UnlockResultDto {
  checked_at: string
  detail: string | null
  id: string
  latency_ms: number | null
  region: string | null
  server_id: string
  service_id: string
  status: string
}

export interface IpQualitySnapshotData {
  as_org: string | null
  asn: string | null
  checked_at: string
  city: string | null
  country: string | null
  ip: string
  ip_type: string
  is_hosting: boolean
  is_proxy: boolean
  is_vpn: boolean
  region: string | null
  risk_level: string
  risk_score: number | null
}

// ---------------------------------------------------------------------------
// API response shapes (mirror crates/server/src/service/ip_quality.rs)
// ---------------------------------------------------------------------------

export interface ServerIpQualityData {
  ip_quality: IpQualitySnapshotData | null
  server_id: string
  unlock_results: UnlockResultDto[]
}

export interface IpQualitySetting {
  check_interval_hours: number
}

export interface UnlockEventDto {
  changed_at: string
  id: string
  new_status: string
  old_status: string
  server_id: string
  service_id: string
}

// ---------------------------------------------------------------------------
// Input shapes (mirror crates/server/src/service/ip_quality.rs)
// ---------------------------------------------------------------------------

export interface CreateCustomServiceInput {
  category: string
  headers: [string, string][]
  method: string
  name: string
  popularity: number
  rules: UnlockRule[]
  timeout_ms: number
  url: string
}

export interface UpdateServiceInput {
  category?: string
  enabled?: boolean
  headers?: [string, string][]
  method?: string
  name?: string
  popularity?: number
  rules?: UnlockRule[]
  timeout_ms?: number
  url?: string
}
