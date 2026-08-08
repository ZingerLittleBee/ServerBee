import { describe, expect, it } from 'vitest'
import { resolveTimeSeriesYDomain } from './y-domain-utils'

describe('resolveTimeSeriesYDomain', () => {
  it('preserves an explicit fixed domain', () => {
    expect(
      resolveTimeSeriesYDomain({
        data: [{ value: 42 }],
        dataKeys: ['value'],
        domain: [0, 100]
      })
    ).toEqual([0, 100])
  })

  it('derives a padded domain and ignores invalid values', () => {
    expect(
      resolveTimeSeriesYDomain({
        data: [{ value: Number.NaN }, { value: 10 }, { value: 'invalid' }],
        dataKeys: ['value']
      })
    ).toEqual([0, 11])
  })
})
