import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { StatNumberWidget } from './stat-number'

const translations: Record<string, string> = {
  stat_servers: 'Servers',
  offline_count: '{{count}} offline',
  avg_cpu: 'Avg CPU',
  avg_memory: 'Avg Memory',
  total_bandwidth: 'Total Bandwidth',
  per_second: '/s',
  healthy: 'Healthy',
  no_data: 'No data',
  online: 'Online',
  servers_online: '{{online}} of {{total}} servers online'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, string | number>) => {
      let template = translations[key] ?? key

      for (const [name, value] of Object.entries(options ?? {})) {
        template = template.replace(`{{${name}}}`, String(value))
      }

      return template
    }
  })
}))

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

describe('StatNumberWidget', () => {
  it('renders the refactored KPI card structure for server counts', () => {
    render(
      <StatNumberWidget
        config={{ label: 'Fleet Coverage', metric: 'server_count', server_id: '' }}
        servers={[makeServer('1'), makeServer('2', { online: false })]}
      />
    )

    expect(screen.getByTestId('stat-number-widget')).toHaveAttribute('data-metric', 'server_count')
    expect(screen.getByTestId('stat-number-label')).toHaveTextContent('Fleet Coverage')
    expect(screen.getByTestId('stat-number-value')).toHaveTextContent('1 / 2')
    expect(screen.getByTestId('stat-number-supporting')).toHaveTextContent('1 offline')
    expect(screen.getByTestId('stat-number-icon-shell')).toBeInTheDocument()
  })

  it('uses translated health copy instead of hardcoded English text', () => {
    render(
      <StatNumberWidget config={{ metric: 'health', server_id: '' }} servers={[makeServer('1', { online: false })]} />
    )

    expect(screen.getByTestId('stat-number-label')).toHaveTextContent('Healthy')
    expect(screen.getByTestId('stat-number-value')).toHaveTextContent('No data')
    expect(screen.getByTestId('stat-number-supporting')).toHaveTextContent('0 of 1 servers online')
  })
})
