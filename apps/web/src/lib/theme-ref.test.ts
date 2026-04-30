import { describe, expect, it } from 'vitest'
import { parseThemeRef, themeRefToString } from './theme-ref'

describe('parseThemeRef', () => {
  it('parses preset theme refs', () => {
    expect(parseThemeRef('preset:default')).toEqual({ kind: 'preset', id: 'default' })
  })

  it('parses custom theme refs', () => {
    expect(parseThemeRef('custom:42')).toEqual({ kind: 'custom', id: 42 })
  })

  it('parses the largest supported custom theme id', () => {
    expect(parseThemeRef('custom:2147483647')).toEqual({ kind: 'custom', id: 2_147_483_647 })
  })

  it('rejects unknown preset ids', () => {
    expect(parseThemeRef('preset:nonsense')).toBeNull()
  })

  it('rejects malformed custom ids', () => {
    for (const ref of [
      'custom:abc',
      'custom:0',
      'custom:-1',
      'custom:+1',
      'custom:1.2',
      'custom:1e2',
      'custom:001',
      'custom:2147483648'
    ]) {
      expect(parseThemeRef(ref)).toBeNull()
    }
  })

  it('rejects empty suffixes', () => {
    expect(parseThemeRef('preset:')).toBeNull()
    expect(parseThemeRef('custom:')).toBeNull()
  })

  it('rejects custom ids with surrounding whitespace', () => {
    expect(parseThemeRef('custom: 1')).toBeNull()
    expect(parseThemeRef('custom:1 ')).toBeNull()
  })

  it('rejects unknown schemes', () => {
    expect(parseThemeRef('foo:bar')).toBeNull()
  })
})

describe('themeRefToString', () => {
  it('round trips theme refs through strings', () => {
    const refs = [
      { kind: 'custom', id: 42 },
      { kind: 'preset', id: 'nord' }
    ] as const

    for (const ref of refs) {
      expect(parseThemeRef(themeRefToString(ref))).toEqual(ref)
    }
  })
})
