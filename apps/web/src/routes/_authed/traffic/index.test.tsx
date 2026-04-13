import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockUseQuery = vi.fn()

let mockOverview: Array<{
  billing_cycle: string | null
  cycle_in: number
  cycle_out: number
  days_remaining: number | null
  name: string
  percent_used: number | null
  server_id: string
  traffic_limit: number | null
}> = []

let mockDailyData: Array<{
  bytes_in: number
  bytes_out: number
  date: string
}> = []

vi.mock('@tanstack/react-query', () => ({
  useQuery: mockUseQuery
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => config
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@/lib/utils', () => ({
  cn: (...classes: Array<string | false | null | undefined>) => classes.filter(Boolean).join(' '),
  formatBytes: (value: number) => `${value} B`
}))

beforeEach(() => {
  vi.clearAllMocks()
  mockOverview = []
  mockDailyData = []

  mockUseQuery.mockImplementation(({ queryKey }: { queryKey: unknown[] }) => {
    if (queryKey[0] === 'traffic' && queryKey[1] === 'overview' && queryKey[2] === 'daily') {
      return { data: mockDailyData, isLoading: false }
    }

    if (queryKey[0] === 'traffic' && queryKey[1] === 'overview') {
      return { data: mockOverview, isLoading: false }
    }

    return { data: undefined, isLoading: false }
  })
})

const { TrafficPage } = await import('./index')

describe('TrafficPage', () => {
  it('shows an explanatory empty state instead of empty stat cards when overview data is missing', () => {
    render(<TrafficPage />)

    expect(screen.getByText('traffic_no_data')).toBeInTheDocument()
    expect(screen.getByText('traffic_configure_prompt')).toBeInTheDocument()
    expect(screen.queryByText('traffic_cycle_inbound')).not.toBeInTheDocument()
    expect(screen.queryByText('traffic_cycle_outbound')).not.toBeInTheDocument()
    expect(screen.queryByText('traffic_highest_usage')).not.toBeInTheDocument()
    expect(screen.queryByText('traffic_servers_warning')).not.toBeInTheDocument()
  })

  it('renders the overview stats and ranking when overview data exists', () => {
    mockOverview = [
      {
        billing_cycle: 'monthly',
        cycle_in: 1024,
        cycle_out: 2048,
        days_remaining: 12,
        name: 'west-monroe-1',
        percent_used: 32.5,
        server_id: 'srv-1',
        traffic_limit: 10_000
      }
    ]

    render(<TrafficPage />)

    expect(screen.getByText('traffic_cycle_inbound')).toBeInTheDocument()
    expect(screen.getByText('traffic_cycle_outbound')).toBeInTheDocument()
    expect(screen.getByText('traffic_highest_usage')).toBeInTheDocument()
    expect(screen.getByText('traffic_servers_warning')).toBeInTheDocument()
    expect(screen.getAllByText('west-monroe-1')).toHaveLength(2)
  })
})
