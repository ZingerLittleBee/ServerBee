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

    // Single pass over entries instead of two reduce() passes; this runs over the
    // full raw record set (a 24h admin window can be tens of thousands of rows).
    let read = 0
    let write = 0
    for (const entry of entries) {
      read += entry.read_bytes_per_sec
      write += entry.write_bytes_per_sec
    }

    return {
      timestamp: record.time,
      read_bytes_per_sec: read,
      write_bytes_per_sec: write
    }
  })
}

export function buildPerDiskIoSeries(records: Pick<ServerMetricRecord, 'disk_io_json' | 'time'>[]): DiskIoSeries[] {
  // Index each record's samples by disk name once so the per-cell lookup below is
  // a Map.get instead of a linear Array.find (O(diskNames * records) -> O(records)).
  const parsedRecords = records.map((record) => {
    const byName = new Map<string, DiskIoSample>()
    for (const entry of parseDiskIoJson(record.disk_io_json)) {
      // Keep the first occurrence to mirror the previous Array.find semantics.
      if (!byName.has(entry.name)) {
        byName.set(entry.name, entry)
      }
    }
    return { timestamp: record.time, byName }
  })

  const diskNames = Array.from(new Set(parsedRecords.flatMap((record) => [...record.byName.keys()]))).toSorted((a, b) =>
    a.localeCompare(b)
  )

  return diskNames.map((name) => ({
    name,
    data: parsedRecords.map((record) => {
      const entry = record.byName.get(name)

      return {
        timestamp: record.timestamp,
        read_bytes_per_sec: entry?.read_bytes_per_sec ?? 0,
        write_bytes_per_sec: entry?.write_bytes_per_sec ?? 0
      }
    })
  }))
}
