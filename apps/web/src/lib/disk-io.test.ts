import { describe, expect, it } from 'vitest'
import type { ServerMetricRecord } from './api-schema'
import { buildMergedDiskIoSeries, buildPerDiskIoSeries, parseDiskIoJson } from './disk-io'

const records = [
  {
    time: '2026-03-19T10:00:00Z',
    disk_io_json:
      '[{"name":"sdb","read_bytes_per_sec":50,"write_bytes_per_sec":150},{"name":"sda","read_bytes_per_sec":100,"write_bytes_per_sec":200}]'
  },
  {
    time: '2026-03-19T11:00:00Z',
    disk_io_json: '[{"name":"sda","read_bytes_per_sec":400,"write_bytes_per_sec":500}]'
  }
] as ServerMetricRecord[]

describe('parseDiskIoJson', () => {
  it('returns an empty array for invalid payloads', () => {
    expect(parseDiskIoJson(undefined)).toEqual([])
    expect(parseDiskIoJson('not-json')).toEqual([])
  })
})

describe('buildMergedDiskIoSeries', () => {
  it('sums all disks per timestamp', () => {
    expect(buildMergedDiskIoSeries(records)).toEqual([
      { timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 150, write_bytes_per_sec: 350 },
      { timestamp: '2026-03-19T11:00:00Z', read_bytes_per_sec: 400, write_bytes_per_sec: 500 }
    ])
  })
})

describe('buildPerDiskIoSeries', () => {
  it('returns stable per-disk series and fills missing timestamps with zeroes', () => {
    expect(buildPerDiskIoSeries(records)).toEqual([
      {
        name: 'sda',
        data: [
          { timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 100, write_bytes_per_sec: 200 },
          { timestamp: '2026-03-19T11:00:00Z', read_bytes_per_sec: 400, write_bytes_per_sec: 500 }
        ]
      },
      {
        name: 'sdb',
        data: [
          { timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 50, write_bytes_per_sec: 150 },
          { timestamp: '2026-03-19T11:00:00Z', read_bytes_per_sec: 0, write_bytes_per_sec: 0 }
        ]
      }
    ])
  })
})
