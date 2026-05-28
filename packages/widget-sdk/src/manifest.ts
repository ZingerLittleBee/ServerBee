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

const SEMVER_RE = /^\d+\.\d+\.\d+(-[\w.]+)?$/
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
