import { describe, expect, it } from 'vitest'
import { isAreaDatumDefined } from './area'

describe('isAreaDatumDefined', () => {
  it('keeps finite zero values while treating missing values as gaps', () => {
    expect(isAreaDatumDefined({ latency: 0 }, 'latency')).toBe(true)
    expect(isAreaDatumDefined({ latency: 12.3 }, 'latency')).toBe(true)
    expect(isAreaDatumDefined({ latency: null }, 'latency')).toBe(false)
    expect(isAreaDatumDefined({}, 'latency')).toBe(false)
    expect(isAreaDatumDefined({ latency: Number.NaN }, 'latency')).toBe(false)
  })
})
