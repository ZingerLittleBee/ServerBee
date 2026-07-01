import type { UnlockRule, UnlockService } from '@/lib/ip-quality-types'

const DEFAULT_TIMEOUT_MS = 5000

export interface ParsedRequest {
  headers: [string, string][]
  method: string
  timeout_ms: number
  url: string
}

export function defaultRule(): UnlockRule {
  return { match: { kind: 'status_equals', code: 200 }, result: 'unlocked' }
}

// Coerce a number-input value to a valid number. `Number(...)` yields `NaN`
// for a partially-typed `-` / `e`, and `NaN` serializes to `null`, which would
// silently corrupt the value, so fall back to 0 instead.
export function toNumber(value: string): number {
  const parsed = Number.parseInt(value, 10)
  return Number.isNaN(parsed) ? 0 : parsed
}

export function parseRequest(service: UnlockService | null | undefined): ParsedRequest {
  const fallback: ParsedRequest = { url: '', method: 'GET', timeout_ms: DEFAULT_TIMEOUT_MS, headers: [] }
  if (!service?.request) {
    return fallback
  }
  try {
    const req = JSON.parse(service.request) as {
      headers?: [string, string][]
      method?: string
      timeout_ms?: number
      url?: string
    }
    return {
      url: req.url ?? '',
      method: req.method ?? 'GET',
      timeout_ms: typeof req.timeout_ms === 'number' ? req.timeout_ms : DEFAULT_TIMEOUT_MS,
      headers: Array.isArray(req.headers) ? req.headers : []
    }
  } catch {
    return fallback
  }
}

export function parseExistingRules(service: UnlockService | null | undefined): UnlockRule[] {
  if (!service?.rules) {
    return [defaultRule()]
  }
  try {
    const parsed = JSON.parse(service.rules) as UnlockRule[]
    return Array.isArray(parsed) && parsed.length > 0 ? parsed : [defaultRule()]
  } catch {
    return [defaultRule()]
  }
}
