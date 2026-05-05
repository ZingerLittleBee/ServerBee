import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CostOverviewResponse, ServerCostInsights } from '@/lib/api-schema'
import { useCostInsights, useCostOverview } from './use-cost'

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false }
    }
  })
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

const costOverview = {
  currencies: [],
  servers: []
} satisfies CostOverviewResponse

const costInsights = {
  server_id: 'srv-1',
  configured: false,
  invalid_reason: 'missing_price',
  price: null,
  currency: 'USD',
  billing_cycle: 'monthly',
  cycle_start: null,
  cycle_end: null,
  cycle_days: null,
  days_elapsed: null,
  days_remaining: null,
  cost_per_second: null,
  cost_per_hour: null,
  cost_per_day: null,
  cost_per_month_equivalent: null,
  cycle_cost_elapsed: null,
  cycle_cost_remaining: null,
  cycle_burn_percent: null,
  resource_value: null,
  value_score: null
} satisfies ServerCostInsights

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('cost hooks', () => {
  it('fetches cost overview with the expected endpoint', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(costOverview))

    const { result } = renderHook(() => useCostOverview(), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(result.current.data?.servers).toEqual([])
    expect(fetch).toHaveBeenCalledWith('/api/cost/overview', expect.any(Object))
  })

  it('fetches cost insights for a server', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(costInsights))

    const { result } = renderHook(() => useCostInsights('srv-1'), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(result.current.data?.server_id).toBe('srv-1')
    expect(fetch).toHaveBeenCalledWith('/api/servers/srv-1/cost-insights', expect.any(Object))
  })

  it('does not fetch cost insights without a server id', () => {
    const { result } = renderHook(() => useCostInsights(''), { wrapper: createWrapper() })

    expect(result.current.fetchStatus).toBe('idle')
    expect(fetch).not.toHaveBeenCalled()
  })
})

function jsonResponse<T>(data: T) {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  })
}
