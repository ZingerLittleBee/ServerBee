import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { parseConfig } from '@/lib/widget-helpers'
import type { DashboardWidget } from '@/lib/widget-types'

type ServerSnapshotKind = 'historical-chart' | 'live' | 'map' | 'name'

type ServerScope =
  | { mode: 'none' }
  | {
      ids: string[] | null
      metric?: string
      mode: 'servers'
      snapshot: ServerSnapshotKind
    }

function readStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return []
  }
  return value.filter((item): item is string => typeof item === 'string' && item.length > 0)
}

function readString(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

function singleServerScope(serverId: unknown, snapshot: ServerSnapshotKind, metric?: string): ServerScope {
  const id = readString(serverId)
  return id.length > 0 ? { mode: 'servers', ids: [id], snapshot, metric } : { mode: 'none' }
}

function selectedServerScope(serverIds: unknown, snapshot: ServerSnapshotKind, metric?: string): ServerScope {
  const ids = readStringArray(serverIds)
  return { mode: 'servers', ids: ids.length > 0 ? ids : null, snapshot, metric }
}

function configuredServerScope(serverIds: unknown, snapshot: ServerSnapshotKind, metric?: string): ServerScope {
  return { mode: 'servers', ids: readStringArray(serverIds), snapshot, metric }
}

function getWidgetServerScope(widget: DashboardWidget): ServerScope {
  const config = parseConfig<Record<string, unknown>>(widget.config_json)

  switch (widget.widget_type) {
    case 'stat-number':
    case 'top-n':
      return { mode: 'servers', ids: null, snapshot: 'live' }
    case 'server-cards':
      return selectedServerScope(config.server_ids, 'live')
    case 'gauge':
      return singleServerScope(config.server_id, 'live')
    case 'line-chart':
      return singleServerScope(config.server_id, 'historical-chart', readString(config.metric))
    case 'multi-line':
      return configuredServerScope(config.server_ids, 'historical-chart', readString(config.metric))
    case 'alert-list':
      return selectedServerScope(config.server_ids, 'name')
    case 'traffic-bar':
    case 'disk-io':
      return singleServerScope(config.server_id, 'name')
    case 'server-map':
      return selectedServerScope(config.server_ids, 'map')
    case 'uptime-timeline':
      return configuredServerScope(config.server_ids, 'name')
    case 'markdown':
    case 'service-status':
      return { mode: 'none' }
    default:
      return { mode: 'servers', ids: null, snapshot: 'live' }
  }
}

function areStringArraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) {
    return false
  }
  return a.every((value, index) => value === b[index])
}

function findServer(servers: ServerMetrics[], id: string): ServerMetrics | undefined {
  return servers.find((server) => server.id === id)
}

function historicalServerSnapshot(server: ServerMetrics, metric?: string): string {
  const parts = [server.id, server.name]
  if (metric === 'memory') {
    parts.push(String(server.mem_total))
  } else if (metric === 'disk') {
    parts.push(String(server.disk_total))
  }
  return parts.join('\u001f')
}

function serverSnapshot(server: ServerMetrics, kind: ServerSnapshotKind, metric?: string): string {
  switch (kind) {
    case 'historical-chart':
      return historicalServerSnapshot(server, metric)
    case 'map':
      return [server.id, server.name, server.country_code ?? ''].join('\u001f')
    case 'name':
      return [server.id, server.name].join('\u001f')
    default:
      return JSON.stringify(server)
  }
}

function serverListSnapshot(
  servers: ServerMetrics[],
  ids: string[],
  snapshot: ServerSnapshotKind,
  metric?: string
): string {
  return ids
    .map((id) => {
      const server = findServer(servers, id)
      return server ? serverSnapshot(server, snapshot, metric) : `${id}\u001fmissing`
    })
    .join('\u001e')
}

function areScopedServersEqual(
  prevServers: ServerMetrics[],
  nextServers: ServerMetrics[],
  scope: ServerScope
): boolean {
  if (scope.mode === 'none') {
    return true
  }

  if (scope.ids === null) {
    const prevIds = prevServers.map((server) => server.id)
    const nextIds = nextServers.map((server) => server.id)
    if (!areStringArraysEqual(prevIds, nextIds)) {
      return false
    }
    return (
      serverListSnapshot(prevServers, prevIds, scope.snapshot, scope.metric) ===
      serverListSnapshot(nextServers, nextIds, scope.snapshot, scope.metric)
    )
  }

  return (
    serverListSnapshot(prevServers, scope.ids, scope.snapshot, scope.metric) ===
    serverListSnapshot(nextServers, scope.ids, scope.snapshot, scope.metric)
  )
}

export function areWidgetServerDependenciesEqual(
  widget: DashboardWidget,
  prevServers: ServerMetrics[],
  nextServers: ServerMetrics[]
): boolean {
  return areScopedServersEqual(prevServers, nextServers, getWidgetServerScope(widget))
}
