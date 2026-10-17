import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { type AlertEvent, type ServiceMonitor, useAlerts, useServiceMonitors, useUptime } from '../src/hooks/domain'

describe('domain hooks', () => {
  let queryClient: QueryClient
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )

  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  })

  afterEach(() => {
    cleanup()
    queryClient.clear()
    vi.unstubAllGlobals()
  })

  it('returns wire uptime rows only after a server is selected', async () => {
    const entries = [
      { date: '2026-10-17', total_minutes: 0, online_minutes: 0, downtime_incidents: 0 },
      { date: '2026-10-18', total_minutes: 60, online_minutes: 45, downtime_incidents: 1 }
    ]
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(Response.json({ data: entries }))
    vi.stubGlobal('fetch', fetch)
    const initialProps: { serverId: string | null } = { serverId: null }
    const { result, rerender } = renderHook(({ serverId }: { serverId: string | null }) => useUptime(serverId, 2), {
      initialProps,
      wrapper
    })

    expect(result.current.fetchStatus).toBe('idle')
    expect(fetch).not.toHaveBeenCalled()

    rerender({ serverId: 'srv-1' })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(fetch).toHaveBeenCalledExactlyOnceWith(
      '/api/servers/srv-1/uptime-daily?days=2',
      expect.objectContaining({ method: 'GET', credentials: 'include' })
    )
    expect(result.current.data).toEqual(entries)
    expect(
      result.current.data?.map(({ date, total_minutes, online_minutes, downtime_incidents }) => [
        date,
        total_minutes,
        online_minutes,
        downtime_incidents
      ])
    ).toEqual([
      ['2026-10-17', 0, 0, 0],
      ['2026-10-18', 60, 45, 1]
    ])

    rerender({ serverId: null })
    expect(result.current.fetchStatus).toBe('idle')
    expect(fetch).toHaveBeenCalledTimes(1)
  })

  it('returns firing and resolved alert events with their wire identities and timestamps', async () => {
    const events = [
      {
        alert_key: 'alert-1',
        rule_id: 'rule-1',
        rule_name: 'High CPU',
        server_id: 'srv-1',
        server_name: 'One',
        status: 'firing',
        event_at: '2026-10-18T00:00:00Z',
        resolved_at: null,
        count: 2
      },
      {
        alert_key: 'alert-2',
        rule_id: 'rule-1',
        rule_name: 'High CPU',
        server_id: 'srv-2',
        server_name: 'Two',
        status: 'resolved',
        event_at: '2026-10-18T01:00:00Z',
        resolved_at: '2026-10-18T01:00:00Z',
        count: 1
      }
    ] satisfies AlertEvent[]
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(Response.json({ data: events }))
    vi.stubGlobal('fetch', fetch)
    const { result } = renderHook(() => useAlerts({ limit: 2 }), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(fetch).toHaveBeenCalledExactlyOnceWith(
      '/api/alert-events?limit=2',
      expect.objectContaining({ method: 'GET', credentials: 'include' })
    )
    expect(result.current.data).toEqual(events)
    expect(result.current.data?.map(({ alert_key, status, resolved_at }) => [alert_key, status, resolved_at])).toEqual([
      ['alert-1', 'firing', null],
      ['alert-2', 'resolved', '2026-10-18T01:00:00Z']
    ])
  })

  it('preserves unknown, healthy and failed monitor states and nullable configuration fields', async () => {
    const unchecked = {
      id: 'monitor-1',
      name: 'Example',
      monitor_type: 'http_keyword',
      target: 'https://example.com',
      interval: 60,
      config_json: '{}',
      notification_group_id: null,
      retry_count: 0,
      server_ids_json: null,
      enabled: true,
      last_status: null,
      consecutive_failures: 0,
      last_checked_at: null,
      created_at: '2026-10-18T00:00:00Z',
      updated_at: '2026-10-18T00:00:00Z'
    }
    const monitors = [
      unchecked,
      {
        ...unchecked,
        id: 'monitor-2',
        last_status: true,
        notification_group_id: 'group-1',
        server_ids_json: '["srv-1"]',
        last_checked_at: '2026-10-18T01:00:00Z'
      },
      {
        ...unchecked,
        id: 'monitor-3',
        last_status: false,
        consecutive_failures: 2,
        last_checked_at: '2026-10-18T01:00:00Z'
      }
    ] satisfies ServiceMonitor[]
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(Response.json({ data: monitors }))
    vi.stubGlobal('fetch', fetch)
    const { result } = renderHook(() => useServiceMonitors(), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(fetch).toHaveBeenCalledExactlyOnceWith(
      '/api/service-monitors',
      expect.objectContaining({ method: 'GET', credentials: 'include' })
    )
    expect(result.current.data).toEqual(monitors)
    expect(
      result.current.data?.map(({ last_status, last_checked_at, notification_group_id, server_ids_json }) => [
        last_status,
        last_checked_at,
        notification_group_id,
        server_ids_json
      ])
    ).toEqual([
      [null, null, null, null],
      [true, '2026-10-18T01:00:00Z', 'group-1', '["srv-1"]'],
      [false, '2026-10-18T01:00:00Z', null, null]
    ])
  })
})
