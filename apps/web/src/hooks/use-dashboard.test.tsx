import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  useCreateDashboard,
  useDashboard,
  useDashboards,
  useDefaultDashboard,
  useDeleteDashboard,
  useUpdateDashboard
} from './use-dashboard'

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

const mockDashboard = {
  id: 'dash-1',
  name: 'My Dashboard',
  is_default: true,
  sort_order: 0,
  created_at: '2026-03-20T00:00:00Z',
  updated_at: '2026-03-20T00:00:00Z'
}

const mockDashboardWithWidgets = {
  ...mockDashboard,
  widgets: [
    {
      id: 'w-1',
      dashboard_id: 'dash-1',
      widget_type: 'stat-number',
      title: 'CPU',
      config_json: '{"metric":"avg_cpu"}',
      grid_x: 0,
      grid_y: 0,
      grid_w: 2,
      grid_h: 2,
      sort_order: 0,
      created_at: '2026-03-20T00:00:00Z'
    }
  ]
}

function mockFetchResponse(body: unknown, options: { status?: number } = {}) {
  const { status = 200 } = options
  return vi.mocked(globalThis.fetch).mockResolvedValueOnce(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' }
    })
  )
}

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useDashboards', () => {
  it('calls GET /api/dashboards', async () => {
    mockFetchResponse({ data: [mockDashboard] })

    const { result } = renderHook(() => useDashboards(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith('/api/dashboards', expect.any(Object))
    expect(result.current.data).toEqual([mockDashboard])
  })
})

describe('useDefaultDashboard', () => {
  it('calls GET /api/dashboards/default', async () => {
    mockFetchResponse({ data: mockDashboardWithWidgets })

    const { result } = renderHook(() => useDefaultDashboard(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith('/api/dashboards/default', expect.any(Object))
    expect(result.current.data).toEqual(mockDashboardWithWidgets)
    expect(result.current.data?.widgets).toHaveLength(1)
  })
})

describe('useDashboard', () => {
  it('calls GET /api/dashboards/:id', async () => {
    mockFetchResponse({ data: mockDashboardWithWidgets })

    const { result } = renderHook(() => useDashboard('dash-1'), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith('/api/dashboards/dash-1', expect.any(Object))
    expect(result.current.data).toEqual(mockDashboardWithWidgets)
  })

  it('does not fetch when id is empty', async () => {
    const { result } = renderHook(() => useDashboard(''), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.fetchStatus).toBe('idle')
    })

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })
})

describe('useCreateDashboard', () => {
  it('calls POST /api/dashboards with name', async () => {
    mockFetchResponse({ data: mockDashboard })

    const { result } = renderHook(() => useCreateDashboard(), { wrapper: createWrapper() })

    result.current.mutate({ name: 'My Dashboard' })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/dashboards',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'My Dashboard' })
      })
    )
  })
})

describe('useUpdateDashboard', () => {
  it('calls PUT /api/dashboards/:id with correct body', async () => {
    mockFetchResponse({ data: mockDashboardWithWidgets })

    const { result } = renderHook(() => useUpdateDashboard(), { wrapper: createWrapper() })

    result.current.mutate({ id: 'dash-1', name: 'Updated Name', widgets: [] })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/dashboards/dash-1',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ name: 'Updated Name', widgets: [] })
      })
    )
  })
})

describe('useDeleteDashboard', () => {
  it('calls DELETE /api/dashboards/:id', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(new Response(null, { status: 204 }))

    const { result } = renderHook(() => useDeleteDashboard(), { wrapper: createWrapper() })

    result.current.mutate('dash-1')

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(globalThis.fetch).toHaveBeenCalledWith(
      '/api/dashboards/dash-1',
      expect.objectContaining({ method: 'DELETE' })
    )
  })
})
