import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { MetricCardWidget } from './metric-card'

const translations: Record<string, string> = {
  'metricCard.metric.cpu': 'CPU',
  'metricCard.metric.memory': 'Memory',
  'metricCard.metric.network': 'Network',
  'metricCard.metric.diskIo': 'Disk I/O',
  'metricCard.past1h': 'past 1h',
  'metricCard.peak': '24H PEAK',
  'metricCard.avg': '24H AVG',
  'metricCard.unknownServer': 'Unknown server'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => translations[key] ?? key
  })
}))

vi.mock('@/hooks/use-api', () => ({
  useServerRecords: () => ({ data: [], isLoading: false })
}))

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'web-1',
    online: true,
    cpu: 42.5,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    disk_used: 0,
    disk_total: 0,
    swap_used: 0,
    swap_total: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_read_speed: 0,
    disk_write_speed: 0,
    ...overrides
  } as unknown as ServerMetrics
}

function wrap(node: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return <QueryClientProvider client={qc}>{node}</QueryClientProvider>
}

describe('MetricCardWidget', () => {
  it('renders the CPU value', () => {
    render(wrap(<MetricCardWidget config={{ metric: 'cpu', server_id: 's1' }} servers={[makeServer()]} />))
    expect(screen.getByTestId('metric-card-value')).toHaveTextContent('42.5%')
  })

  it('shows unknown server placeholder when server_id is missing', () => {
    render(wrap(<MetricCardWidget config={{ metric: 'cpu', server_id: 'missing' }} servers={[makeServer()]} />))
    expect(screen.getByText('Unknown server')).toBeInTheDocument()
  })

  it('renders dash for delta when no history is available', () => {
    render(wrap(<MetricCardWidget config={{ metric: 'cpu', server_id: 's1' }} servers={[makeServer()]} />))
    expect(screen.getByTestId('metric-card-delta')).toHaveTextContent('—')
  })

  it('uses the custom label override', () => {
    render(
      wrap(
        <MetricCardWidget
          config={{ metric: 'memory', server_id: 's1', label: 'RAM Pressure' }}
          servers={[makeServer()]}
        />
      )
    )
    expect(screen.getByText('RAM Pressure')).toBeInTheDocument()
  })
})
