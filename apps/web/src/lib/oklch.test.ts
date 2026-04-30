import { describe, expect, test } from 'vitest'

import { formatOklch, hexToOklch, oklchToHex, parseOklch } from './oklch'

const RED_HEX_ROUND_TRIP_RE = /^#f[ef]0000$/
const WHITE_WITH_TEN_PERCENT_ALPHA_RE = /^#ffffff1a$/

describe('parseOklch', () => {
  test('parses oklch without alpha', () => {
    expect(parseOklch('oklch(0.5 0.1 180)')).toEqual({ l: 0.5, c: 0.1, h: 180 })
  })

  test('parses oklch with outer whitespace', () => {
    expect(parseOklch('  \noklch(0.5 0.1 180)\t ')).toEqual({ l: 0.5, c: 0.1, h: 180 })
  })

  test('parses oklch with numeric alpha', () => {
    expect(parseOklch('oklch(0.5 0.1 180 / 0.5)')).toEqual({
      l: 0.5,
      c: 0.1,
      h: 180,
      alpha: 0.5,
      alphaIsPercent: false
    })
  })

  test('parses oklch with percent alpha', () => {
    expect(parseOklch('oklch(0.5 0.1 180 / 10%)')).toEqual({
      l: 0.5,
      c: 0.1,
      h: 180,
      alpha: 10,
      alphaIsPercent: true
    })
  })

  test('rejects garbage', () => {
    expect(parseOklch('not a color')).toBeNull()
    expect(parseOklch('oklch(0.5, 0.1, 180)')).toBeNull()
  })
})

describe('formatOklch', () => {
  test('formats rounded components and preserves percent alpha', () => {
    expect(formatOklch({ l: 0.555_555, c: 0.123_456, h: 180.987_654, alpha: 10, alphaIsPercent: true })).toBe(
      'oklch(0.5556 0.1235 180.9877 / 10%)'
    )
  })

  test('formats numeric alpha', () => {
    expect(formatOklch({ l: 0.5, c: 0.1, h: 180, alpha: 0.5, alphaIsPercent: false })).toBe('oklch(0.5 0.1 180 / 0.5)')
  })
})

describe('OKLCH and hex conversion', () => {
  test('round-trips formatted OKLCH through parser', () => {
    const formatted = formatOklch({ l: 0.5, c: 0.1, h: 180, alpha: 0.5, alphaIsPercent: false })

    expect(parseOklch(formatted)).toEqual({ l: 0.5, c: 0.1, h: 180, alpha: 0.5, alphaIsPercent: false })
  })

  test('converts OKLCH to hex', () => {
    expect(oklchToHex('oklch(0.62796 0.25768 29.2339)')).toBe('#ff0000')
  })

  test('converts transparent OKLCH to hex with alpha', () => {
    expect(oklchToHex('oklch(1 0 0 / 10%)')?.toLowerCase()).toMatch(WHITE_WITH_TEN_PERCENT_ALPHA_RE)
  })

  test('converts hex to OKLCH', () => {
    const oklch = parseOklch(hexToOklch('#ff0000') ?? '')

    expect(oklch?.l).toBeCloseTo(0.628, 4)
    expect(oklch?.c).toBeCloseTo(0.2577, 4)
    expect(oklch?.h).toBeCloseTo(29.2339, 4)
  })

  test('preserves alpha when converting hex to OKLCH', () => {
    expect(parseOklch(hexToOklch('#ff000080') ?? '')?.alpha).toBeCloseTo(0.502, 3)
  })

  test('round-trips hex through OKLCH within tolerance', () => {
    const oklch = hexToOklch('#ff0000')

    expect(oklch).not.toBeNull()
    expect(oklchToHex(oklch ?? '')).toMatch(RED_HEX_ROUND_TRIP_RE)
  })

  test('returns null for invalid conversion input', () => {
    expect(oklchToHex('not a color')).toBeNull()
    expect(hexToOklch('not a color')).toBeNull()
  })
})
