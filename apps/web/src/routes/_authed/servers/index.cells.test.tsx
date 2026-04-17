import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { MetricBarRow } from './index.cells'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

export function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: null,
    cpu: 0,
    cpu_cores: null,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 500_000_000_000,
    disk_used: 120_000_000_000,
    disk_write_bytes_per_sec: 0,
    features: [],
    group_id: null,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 8_000_000_000,
    mem_used: 3_200_000_000,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

describe('MetricBarRow', () => {
  it('renders green bar below 70%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={50} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-emerald-500/)
  })

  it('renders amber bar at 70% and below 90%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={70.5} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-amber-500/)
  })

  it('renders red bar at 90%+', () => {
    const { container } = render(<MetricBarRow icon={null} pct={92} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-red-500/)
  })

  it('rounds the percentage to 0 decimals', () => {
    render(<MetricBarRow icon={null} pct={42.67} />)
    expect(screen.getByText('43%')).toBeDefined()
  })

  it('clamps percentage to [0, 100]', () => {
    render(<MetricBarRow icon={null} pct={150} />)
    expect(screen.getByText('100%')).toBeDefined()
    render(<MetricBarRow icon={null} pct={-5} />)
    expect(screen.getByText('0%')).toBeDefined()
  })

  it('renders the supplied icon slot', () => {
    render(<MetricBarRow icon={<span data-testid="cpu-icon" />} pct={10} />)
    expect(screen.getByTestId('cpu-icon')).toBeDefined()
  })
})

import { CpuCell } from './index.cells'

describe('CpuCell', () => {
  it('renders cores + load when cpu_cores is present', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: 8, load1: 1.234 })} />)
    expect(screen.getByText('12%')).toBeDefined()
    expect(screen.getByText(/8 cores · load 1\.23/)).toBeDefined()
  })

  it('falls back to load-only when cpu_cores is null (Phase A)', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: null, load1: 1.23 })} />)
    expect(screen.queryByText(/cores/)).toBeNull()
    expect(screen.getByText(/load 1\.23/)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<CpuCell server={makeServer({ online: false, cpu_cores: 8, load1: 1.23 })} />)
    expect(screen.queryByText(/cores/)).toBeNull()
    expect(screen.queryByText(/load/)).toBeNull()
  })
})

import { MemoryCell } from './index.cells'

describe('MemoryCell', () => {
  it('renders used/total + swap pct', () => {
    render(
      <MemoryCell
        server={makeServer({
          mem_used: 7.2 * 1024 ** 3,
          mem_total: 16 * 1024 ** 3,
          swap_used: 0.1 * 1024 ** 3,
          swap_total: 4 * 1024 ** 3
        })}
      />
    )
    expect(screen.getByText(/7\.2 GB \/ 16\.0 GB/)).toBeDefined()
    expect(screen.getByText(/swap/)).toBeDefined()
    expect(screen.getByText(/3%/)).toBeDefined()
  })

  it('renders 0% swap when swap_total is 0', () => {
    render(<MemoryCell server={makeServer({ mem_used: 100, mem_total: 200, swap_used: 0, swap_total: 0 })} />)
    expect(screen.getByText(/swap 0%/)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<MemoryCell server={makeServer({ online: false })} />)
    expect(screen.queryByText(/swap/)).toBeNull()
  })
})
