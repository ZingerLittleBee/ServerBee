import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkProbeRecord, NetworkServerSummary } from '@/lib/network-types'
import { NetworkLatencyWidget } from './network-latency-widget'

const NO_DATA_RE = /no network probe data/i

const recordsMock = vi.fn<() => NetworkProbeRecord[]>()
const summaryMock = vi.fn<() => { data: NetworkServerSummary | undefined }>()

vi.mock('@/hooks/use-network-chart-records', () => ({
  useNetworkChartRecords: () => recordsMock()
}))

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkServerSummary: () => summaryMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

// LatencyChart is exercised in its own context; stub it so this test focuses on the widget shell.
vi.mock('@/components/network/latency-chart', () => ({
  LatencyChart: ({ records, embedded }: { records: NetworkProbeRecord[]; embedded?: boolean }) => (
    <div data-embedded={embedded ? 'true' : 'false'} data-testid="latency-chart">
      {records.length} points
    </div>
  )
}))

const summary: NetworkServerSummary = {
  server_id: 'srv-1',
  server_name: 'Server 1',
  online: true,
  last_probe_at: null,
  anomaly_count: 0,
  latency_sparkline: [],
  loss_sparkline: [],
  targets: [
    {
      target_id: 't-1',
      target_name: 'China Telecom',
      provider: 'ct',
      avg_latency: 20,
      min_latency: 18,
      max_latency: 25,
      packet_loss: 0,
      availability: 100
    }
  ]
}

describe('NetworkLatencyWidget', () => {
  it('renders the latency chart with merged records', () => {
    summaryMock.mockReturnValue({ data: summary })
    recordsMock.mockReturnValue([
      {
        id: 1,
        server_id: 'srv-1',
        target_id: 't-1',
        timestamp: '2026-05-29T10:00:00.000Z',
        avg_latency: 20,
        min_latency: 18,
        max_latency: 25,
        packet_loss: 0,
        packet_sent: 10,
        packet_received: 10
      }
    ])
    render(<NetworkLatencyWidget config={{ server_id: 'srv-1', hours: 24 }} servers={[]} />)
    expect(screen.getByTestId('latency-chart')).toHaveTextContent('1 points')
  })

  it('renders the chart in embedded mode so it fills the widget cell without nested card chrome', () => {
    summaryMock.mockReturnValue({ data: summary })
    recordsMock.mockReturnValue([
      {
        id: 1,
        server_id: 'srv-1',
        target_id: 't-1',
        timestamp: '2026-05-29T10:00:00.000Z',
        avg_latency: 20,
        min_latency: 18,
        max_latency: 25,
        packet_loss: 0,
        packet_sent: 10,
        packet_received: 10
      }
    ])
    render(<NetworkLatencyWidget config={{ server_id: 'srv-1', hours: 24 }} servers={[]} />)
    expect(screen.getByTestId('latency-chart')).toHaveAttribute('data-embedded', 'true')
  })

  it('renders empty state when there are no records', () => {
    summaryMock.mockReturnValue({ data: summary })
    recordsMock.mockReturnValue([])
    render(<NetworkLatencyWidget config={{ server_id: 'srv-1', hours: 24 }} servers={[]} />)
    expect(screen.getByText(NO_DATA_RE)).toBeInTheDocument()
  })
})
