import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CpuCell, MemoryCell } from './index.cells'

const CPU_LOAD_TEXT = /card_load\s+1\.23/

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: null,
    cpu: 45,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 500_000_000_000,
    disk_used: 120_000_000_000,
    disk_write_bytes_per_sec: 0,
    group_id: null,
    last_active: 0,
    load1: 1.23,
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
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

describe('CpuCell', () => {
  it('shows cpu percentage and load1', () => {
    render(<CpuCell server={makeServer({ cpu: 45, load1: 1.23 })} />)
    expect(screen.getByText('45%')).toBeDefined()
    expect(screen.getByText(CPU_LOAD_TEXT)).toBeDefined()
  })
})

describe('MemoryCell', () => {
  it('shows used/total with percentage', () => {
    render(<MemoryCell server={makeServer({ mem_used: 3_200_000_000, mem_total: 8_000_000_000 })} />)
    expect(screen.getByText('3.0 GB / 7.5 GB')).toBeDefined()
    expect(screen.getByText('40%')).toBeDefined()
  })

  it('renders 0B / 0B when mem_total is zero', () => {
    render(<MemoryCell server={makeServer({ mem_used: 0, mem_total: 0 })} />)
    expect(screen.getByText('0 B / 0 B')).toBeDefined()
    expect(screen.getByText('0%')).toBeDefined()
  })
})
