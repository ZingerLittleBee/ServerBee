import { ZodSchema } from './primitives'
import { ZError } from './validate'

class ZServerId extends ZodSchema<string> {
  _kind = 'serverId'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || input.length === 0) {
      throw new ZError(path, 'expected non-empty serverId')
    }
    return input
  }
}

const METRIC_PATH_RE = /^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\])*$/

class ZMetricPath extends ZodSchema<string> {
  _kind = 'metricPath'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !METRIC_PATH_RE.test(input)) {
      throw new ZError(path, 'expected metric path like cpu.usage or disks[0].used')
    }
    return input
  }
}

const COLOR_RE = /^(#[0-9a-fA-F]{3,8}|oklch\([^)]+\)|rgb[a]?\([^)]+\)|hsl[a]?\([^)]+\))$/

class ZColor extends ZodSchema<string> {
  _kind = 'color'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !COLOR_RE.test(input)) {
      throw new ZError(path, 'expected CSS color')
    }
    return input
  }
}

const DURATION_RE = /^\d+(s|m|h|d)$/

class ZDuration extends ZodSchema<string> {
  _kind = 'duration'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !DURATION_RE.test(input)) {
      throw new ZError(path, 'expected duration like 5m / 1h')
    }
    return input
  }
}

export const serverId = () => new ZServerId()
export const metricPath = () => new ZMetricPath()
export const color = () => new ZColor()
export const duration = () => new ZDuration()
