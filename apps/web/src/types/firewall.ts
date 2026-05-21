/**
 * Wire types for the firewall blocklist feature. These mirror the
 * server-side DTOs in `crates/server/src/router/api/firewall.rs` and the
 * shared enum `BlocklistEntryState` in `crates/common/src/firewall.rs`.
 *
 * Once `bun run generate:api-types` is re-run after these endpoints land in
 * the published OpenAPI schema, we can switch to importing from
 * `lib/api-schema.ts` instead.
 */

export type BlocklistEntryState = 'present' | 'absent' | 'failed'

export type BlocklistChangeKind = 'created' | 'deleted'

/** Aliases the server's `BlockListItem` REST DTO. */
export interface BlockListItem {
  comment?: string | null
  /** `all` | `include` | `exclude` */
  cover_type: string
  /** RFC3339 timestamp. */
  created_at: string
  /** User id who created the block (manual origin). */
  created_by?: string | null
  /** `4` or `6`. */
  family: number
  /** uuid */
  id: string
  /** `manual` | `auto` */
  origin: string
  origin_event_id?: string | null
  origin_rule_id?: string | null
  /** Present when `cover_type !== 'all'`. */
  server_ids?: string[] | null
  /** Canonical CIDR (`1.2.3.4/32`, `2001:db8::/32`, etc.) */
  target: string
}

export interface FirewallStats {
  auto: number
  manual: number
  total: number
  v4: number
  v6: number
}

export interface CreateBlockReq {
  comment?: string | null
  /** Defaults to `'all'` on the server. */
  cover_type?: string
  server_ids?: string[] | null
  /** Bare IP or CIDR. Server canonicalizes to a CIDR. */
  target: string
}

export interface BlockListResponse {
  items: BlockListItem[]
  next_cursor: string | null
}

export interface FirewallBlocksFilters {
  limit?: number | null
  origin?: string | null
  target_q?: string | null
}
