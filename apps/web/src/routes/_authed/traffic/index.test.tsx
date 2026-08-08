import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const overviewState = vi.hoisted(() => ({ data: undefined as unknown, isLoading: false }))
const dailyState = vi.hoisted(() => ({ data: undefined as unknown, isLoading: false }))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: unknown) => config
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: ({ queryKey }: { queryKey: string[] }) => (queryKey.includes('daily') ? dailyState : overviewState)
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@/lib/api-client', () => ({
  api: { get: vi.fn() }
}))

// Mirror the real module's full export list: chart.tsx reads Tooltip (and
// renders Legend/ResponsiveContainer) from this module, so a partial mock
// throws the moment chart.tsx is evaluated.
vi.mock('@/components/ui/recharts-lazy', () => ({
  Area: () => null,
  AreaChart: () => null,
  Bar: () => null,
  BarChart: () => null,
  CartesianGrid: () => null,
  Legend: () => null,
  Line: () => null,
  LineChart: () => null,
  ResponsiveContainer: () => null,
  Tooltip: () => null,
  XAxis: () => null,
  YAxis: () => null
}))

async function renderPage() {
  const { TrafficPage } = await import('./index')
  return render(<TrafficPage />)
}

describe('TrafficPage', () => {
  beforeEach(() => {
    overviewState.data = undefined
    overviewState.isLoading = false
    dailyState.data = undefined
    dailyState.isLoading = false
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the generated traffic-overview skeleton while loading', async () => {
    overviewState.isLoading = true
    const { container } = await renderPage()

    const skeleton = container.querySelector('[data-boneyard="traffic-overview"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
    expect(screen.queryByText('traffic_overview_title')).toBeNull()
  })

  it('renders the traffic table once loaded, without any skeleton container', async () => {
    overviewState.data = [
      {
        server_id: 'srv-1',
        name: 'Web Server 01',
        cycle_in: 1_073_741_824,
        cycle_out: 536_870_912,
        percent_used: 12.5,
        traffic_limit: 1_099_511_627_776,
        days_remaining: 21,
        billing_cycle: 'monthly'
      }
    ]
    dailyState.data = []
    const { container } = await renderPage()

    expect(screen.getByText('traffic_overview_title')).toBeInTheDocument()
    expect(screen.getByText('Web Server 01')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('keeps the empty state free of skeleton markup', async () => {
    overviewState.data = []
    dailyState.data = []
    const { container } = await renderPage()

    expect(screen.getByText('traffic_no_data')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })
})
