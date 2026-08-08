import { curveLinear } from '@visx/curve'
import { describe, expect, it } from 'vitest'
import {
  interpolateSeriesPathPoints,
  type SeriesPathPoint,
  seriesAreaPathFromPoints,
  seriesPathFromPoints
} from './series-path-utils'

const samplePoints: SeriesPathPoint[] = [
  { key: 'a', x: 0, y: 10 },
  { key: 'b', x: 10, y: 20 },
  { key: 'c', x: 20, y: 5 }
]

describe('seriesPathFromPoints', () => {
  it('builds an open stroke path', () => {
    const path = seriesPathFromPoints(samplePoints, curveLinear)
    expect(path.startsWith('M')).toBe(true)
    expect(path).toContain('L')
  })
})

describe('seriesAreaPathFromPoints', () => {
  it('builds a closed area path against the baseline', () => {
    const path = seriesAreaPathFromPoints(samplePoints, curveLinear, 40)
    expect(path.startsWith('M')).toBe(true)
    // Closed fill should return to the baseline and end with Z.
    expect(path.toUpperCase().includes('Z')).toBe(true)
  })

  it('returns empty string for empty point lists', () => {
    expect(seriesAreaPathFromPoints([], curveLinear, 40)).toBe('')
  })
})

describe('interpolateSeriesPathPoints', () => {
  it('lerps matching keys between frames', () => {
    const from: SeriesPathPoint[] = [
      { key: 'a', x: 0, y: 0 },
      { key: 'b', x: 10, y: 0 }
    ]
    const to: SeriesPathPoint[] = [
      { key: 'a', x: 0, y: 20 },
      { key: 'b', x: 10, y: 40 }
    ]
    const mid = interpolateSeriesPathPoints(from, to, 0.5)
    expect(mid).toEqual([
      { key: 'a', x: 0, y: 10 },
      { key: 'b', x: 10, y: 20 }
    ])
  })
})
