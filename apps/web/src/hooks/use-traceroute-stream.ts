import { useEffect, useState } from 'react'
import { subscribeBrowserMessage } from '@/hooks/use-servers-ws'
import type { RecordedProtocol, TracerouteHop } from '@/lib/network-types'

export interface TracerouteStreamState {
  completed: boolean
  error: string | null
  hops: TracerouteHop[]
  protocol: RecordedProtocol
  request_id: string
  round: number
  started_at: number
  target: string
  total_rounds: number
}

type BrowserMessage = Record<string, unknown>

function stringOrNull(value: unknown): string | null {
  return typeof value === 'string' ? value : null
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' ? value : null
}

function booleanOrFalse(value: unknown): boolean {
  return typeof value === 'boolean' ? value : false
}

function protocolOrLegacy(value: unknown): RecordedProtocol {
  return value === 'icmp' || value === 'udp' || value === 'tcp' || value === 'legacy' ? value : 'legacy'
}

function isRecord(value: unknown): value is BrowserMessage {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function stringArray(value: unknown): string[] | undefined {
  return Array.isArray(value) ? value.filter((item) => typeof item === 'string') : undefined
}

function optionalNumber(value: unknown): number | null | undefined {
  return value === undefined ? undefined : numberOrNull(value)
}

function optionalString(value: unknown): string | null | undefined {
  return value === undefined ? undefined : stringOrNull(value)
}

function parseHop(value: unknown): TracerouteHop | null {
  if (!isRecord(value) || typeof value.hop !== 'number') {
    return null
  }
  return {
    hop: value.hop,
    hostname: stringOrNull(value.hostname),
    asn: stringOrNull(value.asn),
    ip: optionalString(value.ip),
    ips: stringArray(value.ips),
    avg_ms: optionalNumber(value.avg_ms),
    best_ms: optionalNumber(value.best_ms),
    jitter_ms: optionalNumber(value.jitter_ms),
    loss_pct: optionalNumber(value.loss_pct),
    rtt1: optionalNumber(value.rtt1),
    rtt2: optionalNumber(value.rtt2),
    rtt3: optionalNumber(value.rtt3),
    stddev_ms: optionalNumber(value.stddev_ms),
    total_recv: optionalNumber(value.total_recv),
    total_sent: optionalNumber(value.total_sent),
    worst_ms: optionalNumber(value.worst_ms)
  }
}

function parseHops(value: unknown): TracerouteHop[] {
  return Array.isArray(value) ? value.map(parseHop).filter((hop) => hop !== null) : []
}

function parseTracerouteUpdate(msg: BrowserMessage): TracerouteStreamState | null {
  if (
    typeof msg.request_id !== 'string' ||
    typeof msg.target !== 'string' ||
    typeof msg.started_at !== 'number' ||
    typeof msg.round !== 'number' ||
    typeof msg.total_rounds !== 'number'
  ) {
    return null
  }

  return {
    request_id: msg.request_id,
    target: msg.target,
    protocol: protocolOrLegacy(msg.protocol),
    started_at: msg.started_at,
    round: msg.round,
    total_rounds: msg.total_rounds,
    hops: parseHops(msg.hops),
    completed: booleanOrFalse(msg.completed),
    error: stringOrNull(msg.error)
  }
}

export function useTracerouteStream(serverId: string, requestId: string | null): TracerouteStreamState | null {
  const [data, setData] = useState<TracerouteStreamState | null>(null)

  useEffect(() => {
    if (!requestId) {
      return
    }
    return subscribeBrowserMessage('traceroute_update', (msg: Record<string, unknown>) => {
      if (msg.server_id !== serverId || msg.request_id !== requestId) {
        return
      }
      const next = parseTracerouteUpdate(msg)
      if (next) {
        setData(next)
      }
    })
  }, [serverId, requestId])

  return data?.request_id === requestId ? data : null
}
