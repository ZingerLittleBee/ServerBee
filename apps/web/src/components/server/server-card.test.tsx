import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ServerCard } from './server-card'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...props }: { children?: React.ReactNode; [k: string]: unknown }) => (
    <a data-testid="server-link" href={`/servers/${props.params && (props.params as { id: string }).id}`}>
      {children}
    </a>
  )
}))

const mockNetworkData = vi.fn()
vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: (...args: unknown[]) => mockNetworkData(...args)
}))

function makeServer(overrides: Partial<Parameters<typeof ServerCard>[0]['server']> = {}) {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: 'US',
    os: 'Ubuntu 22.04',
    cpu: 72,
    cpu_name: 'Intel i7',
    mem_used: 4_294_967_296,
    mem_total: 8_589_934_592,
    disk_used: 21_474_836_480,
    disk_total: 53_687_091_200,
    swap_used: 536_870_912,
    swap_total: 2_147_483_648,
    load1: 0.72,
    load5: 0.65,
    load15: 0.58,
    process_count: 142,
    tcp_conn: 38,
    udp_conn: 12,
    uptime: 1_987_200,
    net_in_speed: 12_900_000,
    net_out_speed: 4_300_000,
    net_in_transfer: 1_099_511_627_776,
    net_out_transfer: 549_755_813_888,
    region: null,
    group_id: null,
    last_active: Date.now(),
    ...overrides
  }
}

describe('ServerCard', () => {
  beforeEach(() => {
    mockNetworkData.mockReturnValue({ data: {} })
  })

  it('renders server name', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('test-server')).toBeDefined()
  })

  it('renders three ring charts with CPU, Memory, Disk labels', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('col_cpu')).toBeDefined()
    expect(screen.getByText('col_memory')).toBeDefined()
    expect(screen.getByText('col_disk')).toBeDefined()
  })

  it('renders system metrics row', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('card_load')).toBeDefined()
    expect(screen.getByText('card_processes')).toBeDefined()
    expect(screen.getByText('card_tcp')).toBeDefined()
    expect(screen.getByText('card_udp')).toBeDefined()
    expect(screen.getByText('card_swap')).toBeDefined()
  })

  it('renders network metrics row', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('card_net_in_speed')).toBeDefined()
    expect(screen.getByText('card_net_out_speed')).toBeDefined()
    expect(screen.getByText('card_net_total')).toBeDefined()
    expect(screen.getByText('col_uptime')).toBeDefined()
  })

  it('does not render network quality section when no data', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.queryByLabelText('Latency trend')).toBeNull()
  })

  it('renders network quality bars when probe data exists', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-1': [
          {
            target_id: 'target-1',
            avg_latency: 32,
            packet_loss: 0.002,
            packet_sent: 4,
            packet_received: 4,
            min_latency: 28,
            max_latency: 36,
            timestamp: '2026-03-31T10:00:00Z'
          },
          {
            target_id: 'target-1',
            avg_latency: 45,
            packet_loss: 0.0,
            packet_sent: 4,
            packet_received: 4,
            min_latency: 40,
            max_latency: 50,
            timestamp: '2026-03-31T10:01:00Z'
          }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByLabelText('Latency trend')).toBeDefined()
    expect(screen.getByLabelText('Packet loss trend')).toBeDefined()
  })

  it('sorts network data chronologically across targets', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-a': [
          {
            target_id: 'target-a',
            avg_latency: 100,
            packet_loss: 0,
            packet_sent: 4,
            packet_received: 4,
            min_latency: 90,
            max_latency: 110,
            timestamp: '2026-03-31T10:02:00Z'
          }
        ],
        'target-b': [
          {
            target_id: 'target-b',
            avg_latency: 20,
            packet_loss: 0,
            packet_sent: 4,
            packet_received: 4,
            min_latency: 18,
            max_latency: 22,
            timestamp: '2026-03-31T10:01:00Z'
          }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    // Both targets should produce 2 bars
    const bars = screen.getByLabelText('Latency trend').querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(2)
  })

  it('handles null avg_latency as probe failure', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-1': [
          {
            target_id: 'target-1',
            avg_latency: null,
            packet_loss: 1.0,
            packet_sent: 4,
            packet_received: 0,
            min_latency: null,
            max_latency: null,
            timestamp: '2026-03-31T10:00:00Z'
          }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    // Avg latency should display "-"
    expect(screen.getByText('-')).toBeDefined()
  })

  it('renders StatusBadge', () => {
    render(<ServerCard server={makeServer({ online: false })} />)
    expect(screen.getByText('offline')).toBeDefined()
  })
})
