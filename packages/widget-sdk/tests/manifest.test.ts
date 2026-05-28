import { describe, expect, it } from 'vitest'
import packageJson from '../package.json'
import { SDK_VERSION } from '../src'
import { isCompatible, validateManifest } from '../src/manifest'

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

describe('SDK_VERSION matches package.json', () => {
  it('SDK_VERSION constant is in sync with package.json version', () => {
    expect(SDK_VERSION).toBe(packageJson.version)
  })
})

describe('isCompatible (semver range check)', () => {
  it('caret: ^0.1.0 accepts host 0.1.0 + patch bumps but rejects 0.2.0', () => {
    expect(isCompatible('0.1.0', '^0.1.0')).toBe(true)
    expect(isCompatible('0.1.5', '^0.1.0')).toBe(true)
    expect(isCompatible('0.0.9', '^0.1.0')).toBe(false)
    expect(isCompatible('0.2.0', '^0.1.0')).toBe(false)
  })

  it('caret: ^1.2.3 accepts any 1.x.y >= 1.2.3 but rejects 2.0.0 and 1.2.2', () => {
    expect(isCompatible('1.2.3', '^1.2.3')).toBe(true)
    expect(isCompatible('1.9.0', '^1.2.3')).toBe(true)
    expect(isCompatible('1.2.2', '^1.2.3')).toBe(false)
    expect(isCompatible('2.0.0', '^1.2.3')).toBe(false)
  })

  it('tilde: ~1.2.3 accepts 1.2.x >= 1.2.3 but rejects 1.3.0', () => {
    expect(isCompatible('1.2.3', '~1.2.3')).toBe(true)
    expect(isCompatible('1.2.9', '~1.2.3')).toBe(true)
    expect(isCompatible('1.2.2', '~1.2.3')).toBe(false)
    expect(isCompatible('1.3.0', '~1.2.3')).toBe(false)
  })

  it('exact: 1.2.3 only matches 1.2.3', () => {
    expect(isCompatible('1.2.3', '1.2.3')).toBe(true)
    expect(isCompatible('1.2.4', '1.2.3')).toBe(false)
    expect(isCompatible('1.3.0', '1.2.3')).toBe(false)
  })

  it('returns false for unparseable host or range', () => {
    expect(isCompatible('bogus', '^1.0.0')).toBe(false)
    expect(isCompatible('1.0.0', 'bogus')).toBe(false)
  })
})
