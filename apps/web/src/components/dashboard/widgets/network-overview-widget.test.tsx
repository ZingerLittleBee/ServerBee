import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { NetworkOverviewWidget } from './network-overview-widget'

const NO_DATA_RE = /no network probe data/i

const overviewMock = vi.fn<() => { data: NetworkServerSummary[]; isLoading: boolean }>()

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: () => overviewMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children?: ReactNode }) => <div>{children}</div>
}))

// Render TanStack Router Link as a plain anchor so the widget can be tested in isolation.
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, params }: { children?: ReactNode; to?: string; params?: Record<string, string> }) => (
    <a href={`${to}/${params?.serverId ?? ''}`}>{children}</a>
  )
}))

const summaries: NetworkServerSummary[] = [
  {
    server_id: 'srv-1',
    server_name: 'Server 1',
    online: true,
    last_probe_at: null,
    anomaly_count: 2,
    latency_sparkline: [10, 12],
    loss_sparkline: [0, 0],
    targets: [
      {
        target_id: 't-1',
        target_name: 'CT',
        provider: 'ct',
        avg_latency: 20,
        min_latency: 18,
        max_latency: 25,
        packet_loss: 0.012,
        availability: 99
      }
    ]
  },
  {
    server_id: 'srv-2',
    server_name: 'Server 2',
    online: false,
    last_probe_at: null,
    anomaly_count: 0,
    latency_sparkline: [],
    loss_sparkline: [],
    targets: []
  }
]

describe('NetworkOverviewWidget', () => {
  it('renders one row per server with a link to the network detail page', () => {
    overviewMock.mockReturnValue({ data: summaries, isLoading: false })
    render(<NetworkOverviewWidget config={{}} servers={[]} />)
    expect(screen.getByText('Server 1')).toBeInTheDocument()
    expect(screen.getByText('Server 2')).toBeInTheDocument()
    const link = screen.getByText('Server 1').closest('a')
    expect(link).toHaveAttribute('href', '/network/$serverId/srv-1')
  })

  it('filters to configured server_ids', () => {
    overviewMock.mockReturnValue({ data: summaries, isLoading: false })
    render(<NetworkOverviewWidget config={{ server_ids: ['srv-2'] }} servers={[]} />)
    expect(screen.queryByText('Server 1')).not.toBeInTheDocument()
    expect(screen.getByText('Server 2')).toBeInTheDocument()
  })

  it('renders empty state when there is no data', () => {
    overviewMock.mockReturnValue({ data: [], isLoading: false })
    render(<NetworkOverviewWidget config={{}} servers={[]} />)
    expect(screen.getByText(NO_DATA_RE)).toBeInTheDocument()
  })
})
