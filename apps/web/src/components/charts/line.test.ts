import { describe, expect, it } from 'vitest'
import { isLineDatumDefined } from './line'

describe('isLineDatumDefined', () => {
  it('accepts only finite numeric series values', () => {
    expect(isLineDatumDefined({ value: 12 }, 'value')).toBe(true)
    expect(isLineDatumDefined({ value: Number.NaN }, 'value')).toBe(false)
    expect(isLineDatumDefined({ value: Number.POSITIVE_INFINITY }, 'value')).toBe(false)
    expect(isLineDatumDefined({}, 'value')).toBe(false)
  })
})
