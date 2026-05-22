import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  useCheckNow,
  useCreateService,
  useDeleteService,
  useIpQualityEvents,
  useIpQualityOverview,
  useIpQualityServer,
  useIpQualityServices,
  useIpQualitySetting,
  useUpdateService,
  useUpdateSetting
} from './use-ip-quality-api'

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false }
    }
  })
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children)
  }
}

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

const mockServices = [
  {
    id: 'svc-1',
    key: 'netflix',
    name: 'Netflix',
    category: 'streaming',
    popularity: 100,
    is_builtin: true,
    enabled: true,
    detector: 'netflix',
    request: null,
    rules: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z'
  }
]

const mockSetting = { check_interval_hours: 12 }

const mockOverview = [
  {
    server_id: 'srv-1',
    unlock_results: [
      {
        id: 'r-1',
        server_id: 'srv-1',
        service_id: 'svc-1',
        status: 'unlocked',
        region: null,
        latency_ms: null,
        detail: null,
        checked_at: '2026-01-01T00:00:00Z'
      }
    ],
    ip_quality: null
  }
]

const mockServerData = {
  server_id: 'srv-1',
  unlock_results: [],
  ip_quality: {
    ip: '1.2.3.4',
    asn: 'AS12345',
    as_org: 'Test ISP',
    country: 'US',
    region: null,
    city: null,
    ip_type: 'residential',
    is_proxy: false,
    is_vpn: false,
    is_hosting: false,
    risk_score: null,
    risk_level: 'unknown',
    checked_at: '2026-01-01T00:00:00Z'
  }
}

describe('useIpQualityServices', () => {
  it('queries GET /api/ip-quality/services', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockServices })
    } as Response)

    const { result } = renderHook(() => useIpQualityServices(), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/services',
      expect.objectContaining({ method: 'GET' })
    )
    expect(result.current.data).toHaveLength(1)
    expect(result.current.data?.[0].key).toBe('netflix')
  })
})

describe('useIpQualitySetting', () => {
  it('queries GET /api/ip-quality/settings', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockSetting })
    } as Response)

    const { result } = renderHook(() => useIpQualitySetting(), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/settings',
      expect.objectContaining({ method: 'GET' })
    )
    expect(result.current.data?.check_interval_hours).toBe(12)
  })
})

describe('useIpQualityOverview', () => {
  it('queries GET /api/ip-quality/overview', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockOverview })
    } as Response)

    const { result } = renderHook(() => useIpQualityOverview(), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/overview',
      expect.objectContaining({ method: 'GET' })
    )
    expect(result.current.data).toHaveLength(1)
    expect(result.current.data?.[0].server_id).toBe('srv-1')
  })
})

describe('useIpQualityServer', () => {
  it('queries GET /api/ip-quality/servers/:id', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockServerData })
    } as Response)

    const { result } = renderHook(() => useIpQualityServer('srv-1'), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/servers/srv-1',
      expect.objectContaining({ method: 'GET' })
    )
    expect(result.current.data?.server_id).toBe('srv-1')
    expect(result.current.data?.ip_quality?.ip).toBe('1.2.3.4')
  })

  it('does not fetch when id is empty', async () => {
    const { result } = renderHook(() => useIpQualityServer(''), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.fetchStatus).toBe('idle'))

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })
})

describe('useIpQualityEvents', () => {
  it('queries GET /api/ip-quality/events with server_id param', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: [] })
    } as Response)

    const { result } = renderHook(() => useIpQualityEvents('srv-1'), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/events?server_id=srv-1',
      expect.objectContaining({ method: 'GET' })
    )
  })

  it('does not fetch when serverId is empty', async () => {
    const { result } = renderHook(() => useIpQualityEvents(''), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.fetchStatus).toBe('idle'))

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })
})

describe('useCreateService', () => {
  it('posts to POST /api/ip-quality/services', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockServices[0] })
    } as Response)

    const { result } = renderHook(() => useCreateService(), { wrapper: createWrapper() })

    result.current.mutate({
      name: 'MyService',
      category: 'other',
      popularity: 50,
      url: 'https://example.com',
      method: 'GET',
      headers: [],
      timeout_ms: 5000,
      rules: [{ match: { kind: 'status_equals', code: 200 }, result: 'unlocked' }]
    })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/services',
      expect.objectContaining({ method: 'POST' })
    )
  })
})

describe('useUpdateService', () => {
  it('puts to PUT /api/ip-quality/services/:id', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockServices[0] })
    } as Response)

    const { result } = renderHook(() => useUpdateService(), { wrapper: createWrapper() })

    result.current.mutate({ id: 'svc-1', enabled: false })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/services/svc-1',
      expect.objectContaining({ method: 'PUT' })
    )
  })
})

describe('useDeleteService', () => {
  it('deletes via DELETE /api/ip-quality/services/:id', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: 'ok' })
    } as Response)

    const { result } = renderHook(() => useDeleteService(), { wrapper: createWrapper() })

    result.current.mutate('svc-1')

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/services/svc-1',
      expect.objectContaining({ method: 'DELETE' })
    )
  })
})

describe('useUpdateSetting', () => {
  it('puts to PUT /api/ip-quality/settings', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: { check_interval_hours: 24 } })
    } as Response)

    const { result } = renderHook(() => useUpdateSetting(), { wrapper: createWrapper() })

    result.current.mutate({ check_interval_hours: 24 })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/settings',
      expect.objectContaining({ method: 'PUT' })
    )
  })
})

describe('useCheckNow', () => {
  it('posts to POST /api/ip-quality/servers/:id/check', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: 'ok' })
    } as Response)

    const { result } = renderHook(() => useCheckNow(), { wrapper: createWrapper() })

    result.current.mutate('srv-1')

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/ip-quality/servers/srv-1/check',
      expect.objectContaining({ method: 'POST' })
    )
  })
})
