import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/lib/server-catalog'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, string | number>) => {
      if (key === 'widgets.topN.title') {
        return `Top ${options?.metric ?? ''}`
      }
      if (key === 'widgets.topN.empty.noServers') {
        return 'No online servers'
      }
      if (key === 'common.metrics.cpu') {
        return 'CPU'
      }
      if (key === 'common.labels.server') {
        return 'Server'
      }
      return key
    }
  })
}))

const { TopNWidget } = await import('./top-n')

function makeServer(id: string, overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id,
    name: `Server ${id}`,
    online: true,
    cpu: 42,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    swap_used: 0,
    swap_total: 0,
    disk_used: 20_000_000_000,
    disk_total: 40_000_000_000,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: 1024,
    net_out_speed: 2048,
    net_in_transfer: 1,
    net_out_transfer: 1,
    load1: 0.5,
    load5: 0.4,
    load15: 0.3,
    tcp_conn: 10,
    udp_conn: 5,
    process_count: 100,
    uptime: 86_400,
    country_code: 'US',
    os: 'Linux',
    cpu_name: 'Test CPU',
    last_active: Date.now(),
    region: null,
    group_id: null,
    ...overrides
  }
}

describe('TopNWidget', () => {
  it('ranks online servers by the selected metric with proportional fills', () => {
    render(
      <TopNWidget
        config={{ metric: 'cpu', count: 3, sort: 'desc' }}
        servers={[
          makeServer('a', { cpu: 10, name: 'Low' }),
          makeServer('b', { cpu: 90, name: 'High' }),
          makeServer('c', { cpu: 50, name: 'Mid' }),
          makeServer('d', { online: false, cpu: 99, name: 'Offline' })
        ]}
      />
    )

    expect(screen.getByRole('heading', { name: 'Top CPU' })).toBeInTheDocument()

    const rows = screen.getAllByTestId('top-n-row')
    expect(rows).toHaveLength(3)
    expect(rows[0]).toHaveTextContent('High')
    expect(rows[0]).toHaveTextContent('90.0%')
    expect(rows[1]).toHaveTextContent('Mid')
    expect(rows[2]).toHaveTextContent('Low')

    // Percent metrics fill against 100%.
    const fills = screen.getAllByTestId('top-n-fill')
    expect(fills[0]).toHaveStyle({ width: '90%' })
    expect(fills[2]).toHaveStyle({ width: '10%' })
  })

  it('tints the rows with the configured color when provided', () => {
    render(
      <TopNWidget
        config={{ metric: 'cpu', count: 2, color: '#ff6600' }}
        servers={[makeServer('a', { cpu: 80 }), makeServer('b', { cpu: 40 })]}
      />
    )

    expect(screen.getAllByTestId('top-n-fill')[0].style.backgroundColor).toContain('rgb(255, 102, 0)')
  })

  it('shows an empty state when no online servers are available', () => {
    render(<TopNWidget config={{ metric: 'cpu' }} servers={[makeServer('offline', { online: false })]} />)

    expect(screen.getByText('No online servers')).toBeInTheDocument()
    expect(screen.queryByTestId('top-n-rows')).not.toBeInTheDocument()
  })

  it('scales open-ended metrics against the largest row', () => {
    render(
      <TopNWidget
        config={{ metric: 'bandwidth', count: 2 }}
        servers={[
          makeServer('a', { net_in_speed: 1000, net_out_speed: 3000 }),
          makeServer('b', { net_in_speed: 500, net_out_speed: 500 })
        ]}
      />
    )

    const fills = screen.getAllByTestId('top-n-fill')
    expect(fills[0]).toHaveStyle({ width: '100%' })
    expect(fills[1]).toHaveStyle({ width: '25%' })
  })
})
