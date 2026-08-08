import { describe, expect, it } from 'vitest'
import {
  ALPHA2_TO_ALPHA3,
  alpha2ToAlpha3,
  alpha3ToAlpha2,
  buildCountryServerGroups,
  countryServerFill
} from './server-geo'

describe('alpha2ToAlpha3', () => {
  it('maps common codes', () => {
    expect(alpha2ToAlpha3('US')).toBe('USA')
    expect(alpha2ToAlpha3('CN')).toBe('CHN')
    expect(alpha2ToAlpha3('DE')).toBe('DEU')
    expect(alpha2ToAlpha3('JP')).toBe('JPN')
  })

  it('normalizes lowercase input', () => {
    expect(alpha2ToAlpha3('us')).toBe('USA')
  })

  it('returns undefined for missing or unknown codes', () => {
    expect(alpha2ToAlpha3(null)).toBeUndefined()
    expect(alpha2ToAlpha3(undefined)).toBeUndefined()
    expect(alpha2ToAlpha3('ZZ')).toBeUndefined()
  })
})

describe('alpha3ToAlpha2', () => {
  it('maps back to an alpha-2 that resolves to the same alpha-3', () => {
    // Alias entries (UK→GBR) mean the reverse map holds one alpha-2 per
    // alpha-3, so assert consistency rather than identity.
    for (const alpha3 of Object.values(ALPHA2_TO_ALPHA3)) {
      expect(alpha2ToAlpha3(alpha3ToAlpha2(alpha3))).toBe(alpha3)
    }
  })

  it('resolves common reverse codes', () => {
    expect(alpha3ToAlpha2('USA')).toBe('US')
    expect(alpha3ToAlpha2('CHN')).toBe('CN')
    expect(alpha3ToAlpha2('DEU')).toBe('DE')
  })
})

describe('buildCountryServerGroups', () => {
  it('groups servers by alpha-3 country id', () => {
    const groups = buildCountryServerGroups([
      { country_code: 'US', name: 'web-1' },
      { country_code: 'us', name: 'web-2' },
      { country_code: 'DE', name: 'fra-1' }
    ])

    expect(groups.get('USA')).toEqual({ alpha2: 'US', count: 2, serverNames: ['web-1', 'web-2'] })
    expect(groups.get('DEU')).toEqual({ alpha2: 'DE', count: 1, serverNames: ['fra-1'] })
  })

  it('skips servers without a mappable country code', () => {
    const groups = buildCountryServerGroups([
      { country_code: null, name: 'local' },
      { country_code: 'ZZ', name: 'nowhere' }
    ])

    expect(groups.size).toBe(0)
  })
})

describe('countryServerFill', () => {
  it('returns muted for countries without servers', () => {
    expect(countryServerFill(undefined, 4)).toBe('var(--muted)')
    expect(countryServerFill(0, 4)).toBe('var(--muted)')
  })

  it('buckets counts relative to the max', () => {
    expect(countryServerFill(4, 4)).toBe('var(--chart-scale-05)')
    expect(countryServerFill(3, 4)).toBe('var(--chart-scale-04)')
    expect(countryServerFill(2, 4)).toBe('var(--chart-scale-03)')
    expect(countryServerFill(1, 4)).toBe('var(--chart-scale-02)')
  })

  it('uses the darkest bucket when all countries tie', () => {
    expect(countryServerFill(1, 1)).toBe('var(--chart-scale-05)')
  })
})
