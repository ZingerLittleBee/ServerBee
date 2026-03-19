import type { ServerMetricRecord } from './api-schema'

export interface DiskIoSample {
  name: string
  read_bytes_per_sec: number
  write_bytes_per_sec: number
}

export interface DiskIoChartPoint {
  read_bytes_per_sec: number
  timestamp: string
  write_bytes_per_sec: number
}

export interface DiskIoSeries {
  data: DiskIoChartPoint[]
  name: string
}

function toNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

export function parseDiskIoJson(raw: string | null | undefined): DiskIoSample[] {
  if (!raw) {
    return []
  }

  try {
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }

    return parsed
      .filter((entry): entry is Record<string, unknown> => typeof entry === 'object' && entry !== null)
      .map((entry) => ({
        name: typeof entry.name === 'string' ? entry.name : '',
        read_bytes_per_sec: toNumber(entry.read_bytes_per_sec),
        write_bytes_per_sec: toNumber(entry.write_bytes_per_sec)
      }))
      .filter((entry) => entry.name.length > 0)
      .sort((left, right) => left.name.localeCompare(right.name))
  } catch {
    return []
  }
}

export function buildMergedDiskIoSeries(
  records: Pick<ServerMetricRecord, 'disk_io_json' | 'time'>[]
): DiskIoChartPoint[] {
  return records.map((record) => {
    const entries = parseDiskIoJson(record.disk_io_json)

    return {
      timestamp: record.time,
      read_bytes_per_sec: entries.reduce((total, entry) => total + entry.read_bytes_per_sec, 0),
      write_bytes_per_sec: entries.reduce((total, entry) => total + entry.write_bytes_per_sec, 0)
    }
  })
}

export function buildPerDiskIoSeries(records: Pick<ServerMetricRecord, 'disk_io_json' | 'time'>[]): DiskIoSeries[] {
  const parsedRecords = records.map((record) => ({
    timestamp: record.time,
    entries: parseDiskIoJson(record.disk_io_json)
  }))

  const diskNames = [...new Set(parsedRecords.flatMap((record) => record.entries.map((entry) => entry.name)))].sort(
    (a, b) => a.localeCompare(b)
  )

  return diskNames.map((name) => ({
    name,
    data: parsedRecords.map((record) => {
      const entry = record.entries.find((sample) => sample.name === name)

      return {
        timestamp: record.timestamp,
        read_bytes_per_sec: entry?.read_bytes_per_sec ?? 0,
        write_bytes_per_sec: entry?.write_bytes_per_sec ?? 0
      }
    })
  }))
}
