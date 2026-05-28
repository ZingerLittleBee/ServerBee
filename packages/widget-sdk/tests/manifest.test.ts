import { describe, expect, it } from 'vitest'
import { validateManifest } from '../src/manifest'

describe('validateManifest', () => {
  const base = {
    id: 'com.example.foo',
    version: '1.0.0',
    name: 'Foo',
    category: 'Real-time' as const,
    sizing: { defaultW: 3, defaultH: 3, minW: 2, minH: 2, strategy: 'aspect-square' as const },
    sdkVersion: '^0.1.0'
  }

  it('accepts a minimal valid manifest', () => {
    expect(validateManifest(base)).toEqual(base)
  })

  it('rejects missing id', () => {
    expect(() => validateManifest({ ...base, id: '' })).toThrow(/id/)
  })

  it('rejects unknown sizing strategy', () => {
    expect(() => validateManifest({ ...base, sizing: { ...base.sizing, strategy: 'bogus' as any } })).toThrow(
      /strategy/
    )
  })

  it('rejects invalid semver', () => {
    expect(() => validateManifest({ ...base, version: 'not-semver' })).toThrow(/version/)
  })
})
