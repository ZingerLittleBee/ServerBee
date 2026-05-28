export type WidgetCategory = 'Real-time' | 'Charts' | 'Status'

export type SizingStrategy = 'fixed' | 'free' | 'aspect-square' | 'content-height'

export interface WidgetSizing {
  defaultH: number
  defaultW: number
  maxH?: number
  maxW?: number
  minH: number
  minW: number
  strategy: SizingStrategy
}

export interface WidgetManifest {
  author?: string
  category: WidgetCategory
  description?: string
  id: string
  name: string
  requiredCaps?: string[]
  sdkVersion: string
  sizing: WidgetSizing
  version: string
}

const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)(-[\w.]+)?$/
const SEMVER_RANGE_RE = /^[\^~]?\d+\.\d+\.\d+/
const VALID_CATEGORIES = new Set<WidgetCategory>(['Real-time', 'Charts', 'Status'])
const VALID_STRATEGIES = new Set<SizingStrategy>(['fixed', 'free', 'aspect-square', 'content-height'])

export function validateManifest(input: unknown): WidgetManifest {
  if (!input || typeof input !== 'object') {
    throw new Error('manifest must be an object')
  }
  const m = input as Record<string, any>

  if (typeof m.id !== 'string' || m.id.length === 0) {
    throw new Error('manifest.id required')
  }
  if (typeof m.version !== 'string' || !SEMVER_RE.test(m.version)) {
    throw new Error('manifest.version must be valid semver')
  }
  if (typeof m.name !== 'string' || m.name.length === 0) {
    throw new Error('manifest.name required')
  }
  if (!VALID_CATEGORIES.has(m.category)) {
    throw new Error('manifest.category invalid')
  }
  if (!m.sizing || typeof m.sizing !== 'object') {
    throw new Error('manifest.sizing required')
  }

  const sz = m.sizing as Record<string, any>
  for (const k of ['defaultW', 'defaultH', 'minW', 'minH'] as const) {
    if (typeof sz[k] !== 'number') {
      throw new Error(`manifest.sizing.${k} must be number`)
    }
  }
  if (!VALID_STRATEGIES.has(sz.strategy)) {
    throw new Error('manifest.sizing.strategy invalid')
  }

  if (typeof m.sdkVersion !== 'string' || !SEMVER_RANGE_RE.test(m.sdkVersion)) {
    throw new Error('manifest.sdkVersion must be valid semver range')
  }

  if (m.requiredCaps !== undefined && !Array.isArray(m.requiredCaps)) {
    throw new Error('manifest.requiredCaps must be array')
  }

  return m as WidgetManifest
}

interface ParsedVersion {
  major: number
  minor: number
  patch: number
}

function parseVersion(v: string): ParsedVersion | null {
  const m = SEMVER_RE.exec(v)
  if (!m) {
    return null
  }
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]) }
}

/**
 * Minimal semver-range compatibility check. Supports:
 *   - exact:   "1.2.3"          → host must be exactly 1.2.3
 *   - caret:   "^1.2.3"         → host >= 1.2.3 and host.major === range.major
 *                                (when major === 0: host.minor must equal range.minor too)
 *   - tilde:   "~1.2.3"         → host >= 1.2.3 and host.major.minor === range.major.minor
 *
 * Pre-release tags on either side are ignored for compatibility comparison.
 */
export function isCompatible(hostVersion: string, range: string): boolean {
  const host = parseVersion(hostVersion)
  if (!host) {
    return false
  }
  const op = range[0]
  const versionPart = op === '^' || op === '~' ? range.slice(1) : range
  const target = parseVersion(versionPart)
  if (!target) {
    return false
  }
  const hostGTE =
    host.major > target.major ||
    (host.major === target.major && host.minor > target.minor) ||
    (host.major === target.major && host.minor === target.minor && host.patch >= target.patch)
  if (op === '^') {
    if (!hostGTE) {
      return false
    }
    if (target.major !== 0) {
      return host.major === target.major
    }
    // ^0.x.y is pinned to 0.x — minor bump is treated as breaking
    if (target.minor !== 0) {
      return host.major === 0 && host.minor === target.minor
    }
    // ^0.0.z is pinned to exact patch
    return host.major === 0 && host.minor === 0 && host.patch === target.patch
  }
  if (op === '~') {
    return hostGTE && host.major === target.major && host.minor === target.minor
  }
  // exact match
  return host.major === target.major && host.minor === target.minor && host.patch === target.patch
}
