import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { GaugeWidget } from './gauge'

function makeServer(id: string, overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id,
    name: `Server ${id}`,
    online: true,
    cpu: 50,
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

function getStops(container: HTMLElement): { start: string | null; end: string | null } {
  const gradient = container.querySelector('[data-testid="gauge-gradient"]')
  if (!gradient) {
    return { start: null, end: null }
  }
  const stops = gradient.querySelectorAll('stop')
  return {
    start: stops[0]?.getAttribute('stop-color') ?? null,
    end: stops[1]?.getAttribute('stop-color') ?? null
  }
}

describe('GaugeWidget', () => {
  it('renders the empty state when the configured server is not in the list', () => {
    render(<GaugeWidget config={{ metric: 'cpu', server_id: 'missing' }} servers={[makeServer('1')]} />)

    expect(screen.getByText('Server not found')).toBeInTheDocument()
    expect(screen.queryByTestId('gauge-svg')).not.toBeInTheDocument()
  })

  it('renders label, formatted value, and server-name subtitle', () => {
    const { container } = render(
      <GaugeWidget
        config={{ label: 'CPU Usage', metric: 'cpu', server_id: '1' }}
        servers={[makeServer('1', { cpu: 50 })]}
      />
    )

    expect(screen.getByTestId('gauge-label')).toHaveTextContent('CPU Usage')
    expect(screen.getByTestId('gauge-value')).toHaveTextContent('50.0%')
    expect(screen.getByTestId('gauge-subtitle')).toHaveTextContent('Server 1')
    expect(container.querySelector('[data-testid="gauge-svg"]')).not.toBeNull()
  })

  it('uses the normal-range gradient (chart-1 → chart-2) when value < 70', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 50 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-1)',
      end: 'var(--chart-2)'
    })
  })

  it('uses the warning gradient (chart-3 → chart-5) when value is in [70, 90)', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 75 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-3)',
      end: 'var(--chart-5)'
    })
  })

  it('uses the critical gradient (chart-4 → chart-3) when value >= 90', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 95 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-4)',
      end: 'var(--chart-3)'
    })
  })

  it('clamps values above the configured max', () => {
    render(<GaugeWidget config={{ max: 80, metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 95 })]} />)

    expect(screen.getByTestId('gauge-value')).toHaveTextContent('80.0%')
  })

  it('hides the progress arc and end-cap balls when value is zero', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 0 })]} />
    )

    expect(container.querySelector('[data-testid="gauge-progress"]')).toBeNull()
    expect(container.querySelector('[data-testid="gauge-endcaps"]')).toBeNull()
    expect(container.querySelector('[data-testid="gauge-track"]')).not.toBeNull()
  })
})
