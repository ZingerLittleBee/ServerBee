/**
 * Navigation policy for the server detail surface, admin and public.
 *
 * One module owns which tabs a detail URL may name, which historical ranges
 * each variant offers, how a range key resolves, and where the public network
 * detail experience lives for a given status-page config (ADR-0001: it is a
 * server-detail tab whenever `show_server_detail` allows one, and the
 * standalone `/status/network/$serverId` page only survives as a fallback for
 * `show_server_detail=false` + `show_network=true` deployments).
 */

// --- Tab catalogs ------------------------------------------------------------

export const SERVER_DETAIL_TABS = ['metrics', 'network', 'traffic', 'security', 'ip-quality'] as const
export type ServerDetailTab = (typeof SERVER_DETAIL_TABS)[number]

export const PUBLIC_SERVER_DETAIL_TABS = ['metrics', 'network'] as const
export type PublicServerDetailTab = (typeof PUBLIC_SERVER_DETAIL_TABS)[number]

/** Admin detail search: `range` always present (default realtime); `tab` kept
 * optional so old `/servers/$id` links (range only) stay valid. */
export function parseServerDetailSearch(search: Record<string, unknown>): { range: RangeKey; tab?: ServerDetailTab } {
  const range = isRangeKey(search.range) ? search.range : 'realtime'
  return SERVER_DETAIL_TABS.includes(search.tab as ServerDetailTab)
    ? { range, tab: search.tab as ServerDetailTab }
    : { range }
}

/** Public detail search: both params optional; unknown ranges and tabs are dropped. */
export function parsePublicServerDetailSearch(search: Record<string, unknown>): {
  range?: RangeKey
  tab?: PublicServerDetailTab
} {
  return {
    range: isRangeKey(search.range) ? search.range : undefined,
    ...(PUBLIC_SERVER_DETAIL_TABS.includes(search.tab as PublicServerDetailTab)
      ? { tab: search.tab as PublicServerDetailTab }
      : {})
  }
}

/** A `?tab=network` deep link falls back to metrics when the network trigger
 * is hidden, instead of selecting a tab that does not exist. */
export function resolvePublicActiveTab(
  tab: PublicServerDetailTab | undefined,
  networkTabAvailable: boolean
): PublicServerDetailTab {
  return networkTabAvailable && tab === 'network' ? 'network' : 'metrics'
}

// --- Range catalogs ----------------------------------------------------------

const RANGE_KEYS = ['realtime', '1h', '6h', '24h', '7d', '30d'] as const
export type RangeKey = (typeof RANGE_KEYS)[number]

function isRangeKey(value: unknown): value is RangeKey {
  return (RANGE_KEYS as readonly unknown[]).includes(value)
}

export interface TimeRange {
  hours: number
  interval: string
  key: RangeKey
  label: string
}

const ADMIN_TIME_RANGES: TimeRange[] = [
  { key: 'realtime', label: 'range_realtime', hours: 0, interval: 'realtime' },
  { key: '1h', label: 'range_1h', hours: 1, interval: 'raw' },
  { key: '6h', label: 'range_6h', hours: 6, interval: 'raw' },
  { key: '24h', label: 'range_24h', hours: 24, interval: 'raw' },
  { key: '7d', label: 'range_7d', hours: 168, interval: 'hourly' },
  { key: '30d', label: 'range_30d', hours: 720, interval: 'hourly' }
]

// Public variant cannot rely on WS-driven realtime metrics, so realtime is
// dropped; everything else mirrors the admin range options because the
// public metrics endpoint accepts the same `interval` query parameter.
const PUBLIC_TIME_RANGES: TimeRange[] = ADMIN_TIME_RANGES.filter((r) => r.key !== 'realtime')

export function rangesForVariant(variant: 'admin' | 'public'): TimeRange[] {
  return variant === 'public' ? PUBLIC_TIME_RANGES : ADMIN_TIME_RANGES
}

/** Resolve a range key against a catalog, falling back to the first entry so
 * an unknown or absent key still renders a chart instead of nothing. */
export function resolveRange(rangeKey: string | undefined, ranges: TimeRange[]) {
  const idx = ranges.findIndex((tr) => tr.key === rangeKey)
  const rangeIndex = idx >= 0 ? idx : 0
  return { range: ranges[rangeIndex], rangeIndex }
}

/** The retired standalone network page counted hours in its `range` param
 * ('1' | '6' | ...); detail URLs use metrics-style keys. Used by the legacy
 * `/network/$serverId` redirect so old bookmarks land on the right window. */
export function legacyNetworkRangeToRangeKey(range: string): RangeKey {
  const mapping: Record<string, RangeKey> = {
    realtime: 'realtime',
    '1': '1h',
    '6': '6h',
    '24': '24h',
    '168': '7d',
    '720': '30d'
  }
  return mapping[range] ?? 'realtime'
}

// --- Public network home (ADR-0001) -------------------------------------------

interface PublicStatusToggles {
  show_network?: boolean | null
  show_server_detail?: boolean | null
}

export type PublicNetworkHome = 'hidden' | 'standalone' | 'tab'

/**
 * Where the public network detail lives for this deployment. Returns
 * `undefined` while the status config is still loading so callers can defer
 * redirects instead of flashing the wrong page.
 *
 * - `tab` — inside `/status/server/$serverId?tab=network` (canonical home)
 * - `standalone` — `/status/network/$serverId` fallback when the server
 *   detail page is disabled but network stays visible
 * - `hidden` — network detail is not exposed at all
 */
export function publicNetworkHome(config: PublicStatusToggles | undefined): PublicNetworkHome | undefined {
  if (config === undefined) {
    return undefined
  }
  if (config.show_network === false) {
    return 'hidden'
  }
  if (config.show_server_detail === false) {
    return 'standalone'
  }
  return 'tab'
}
