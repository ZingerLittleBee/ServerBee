import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { NetworkQualityWidget } from './network-quality'

const NO_DATA_RE = /no network probe data/i

const summaryMock = vi.fn<() => { data: NetworkServerSummary | undefined; isLoading: boolean }>()

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkServerSummary: () => summaryMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children?: ReactNode }) => <div>{children}</div>
}))

const baseSummary: NetworkServerSummary = {
  server_id: 'srv-1',
  server_name: 'Server 1',
  online: true,
  last_probe_at: '2026-05-29T10:00:00.000Z',
  anomaly_count: 0,
  latency_sparkline: [],
  loss_sparkline: [],
  targets: [
    {
      target_id: 't-1',
      target_name: 'China Telecom',
      provider: 'ct',
      avg_latency: 23.1,
      min_latency: 20,
      max_latency: 30,
      packet_loss: 0,
      availability: 100
    },
    {
      target_id: 't-2',
      target_name: 'International',
      provider: 'international',
      avg_latency: 142.3,
      min_latency: 130,
      max_latency: 160,
      packet_loss: 0.015,
      availability: 98
    }
  ]
}

describe('NetworkQualityWidget', () => {
  it('renders each target with latency and packet loss', () => {
    summaryMock.mockReturnValue({ data: baseSummary, isLoading: false })
    render(<NetworkQualityWidget config={{ server_id: 'srv-1' }} servers={[]} />)
    expect(screen.getByText('China Telecom')).toBeInTheDocument()
    expect(screen.getByText('International')).toBeInTheDocument()
    expect(screen.getByText('23.1 ms')).toBeInTheDocument()
  })

  it('renders empty state when there are no targets', () => {
    summaryMock.mockReturnValue({ data: { ...baseSummary, targets: [] }, isLoading: false })
    render(<NetworkQualityWidget config={{ server_id: 'srv-1' }} servers={[]} />)
    expect(screen.getByText(NO_DATA_RE)).toBeInTheDocument()
  })
})
