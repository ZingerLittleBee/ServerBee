import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
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

vi.mock('recharts', () => {
  const createWrapper =
    (testId: string) =>
    ({ children }: { children?: React.ReactNode }) => <div data-testid={testId}>{children}</div>

  return {
    ResponsiveContainer: createWrapper('responsive-container'),
    Tooltip: createWrapper('chart-tooltip'),
    Legend: createWrapper('chart-legend'),
    CartesianGrid: () => <div data-testid="cartesian-grid" />,
    XAxis: () => <div data-testid="x-axis" />,
    YAxis: () => <div data-testid="y-axis" />,
    BarChart: createWrapper('bar-chart'),
    Bar: ({ children, dataKey }: { children?: React.ReactNode; dataKey: string }) => (
      <div data-testid={`bar-${dataKey}`}>{children}</div>
    ),
    Cell: ({ fill }: { fill?: string }) => <div data-fill={fill} data-testid="bar-cell" />
  }
})

const mockNetworkOverview = vi.fn()
const mockNetworkRealtime = vi.fn()
vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: (...args: unknown[]) => mockNetworkOverview(...args)
}))
vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: (...args: unknown[]) => mockNetworkRealtime(...args)
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

function makeSummary(overrides: Partial<NetworkServerSummary> = {}): NetworkServerSummary {
  return {
    anomaly_count: 0,
    last_probe_at: null,
    latency_sparkline: [],
    loss_sparkline: [],
    online: true,
    server_id: 'srv-1',
    server_name: 'test-server',
    targets: [],
    ...overrides
  }
}

describe('ServerCard', () => {
  beforeEach(() => {
    mockNetworkOverview.mockReturnValue({ data: [] })
    mockNetworkRealtime.mockReturnValue({ data: {} })
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
    expect(screen.queryByText('card_network_quality')).toBeNull()
  })

  it('renders a single aggregated latency and loss view from target summaries', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          targets: [
            {
              availability: 0.99,
              avg_latency: 40,
              max_latency: 45,
              min_latency: 35,
              packet_loss: 0.01,
              provider: 'ct',
              target_id: 'target-1',
              target_name: 'Shanghai Telecom'
            },
            {
              availability: 0.97,
              avg_latency: 80,
              max_latency: 90,
              min_latency: 70,
              packet_loss: 0.03,
              provider: 'cu',
              target_id: 'target-2',
              target_name: 'Beijing Unicom'
            }
          ]
        })
      ]
    })

    render(<ServerCard server={makeServer()} />)

    expect(screen.getByText('60ms')).toBeDefined()
    expect(screen.getByLabelText('Latency trend')).toBeDefined()
    expect(screen.queryByLabelText('Packet loss trend')).toBeNull()
    expect(screen.queryByText('Shanghai Telecom')).toBeNull()
    expect(screen.queryByText('Beijing Unicom')).toBeNull()
  })

  it('uses realtime samples to update the aggregated current values', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          targets: [
            {
              availability: 0.99,
              avg_latency: 40,
              max_latency: 45,
              min_latency: 35,
              packet_loss: 0.01,
              provider: 'ct',
              target_id: 'target-1',
              target_name: 'Shanghai'
            },
            {
              availability: 0.98,
              avg_latency: 50,
              max_latency: 55,
              min_latency: 45,
              packet_loss: 0.02,
              provider: 'cu',
              target_id: 'target-2',
              target_name: 'Beijing'
            }
          ]
        })
      ]
    })
    mockNetworkRealtime.mockReturnValue({
      data: {
        'target-1': [
          {
            avg_latency: 50,
            max_latency: 55,
            min_latency: 45,
            packet_loss: 0.02,
            packet_received: 9,
            packet_sent: 10,
            target_id: 'target-1',
            timestamp: '2026-04-12T10:00:00Z'
          },
          {
            avg_latency: 70,
            max_latency: 75,
            min_latency: 65,
            packet_loss: 0.04,
            packet_received: 8,
            packet_sent: 10,
            target_id: 'target-1',
            timestamp: '2026-04-12T10:01:00Z'
          }
        ],
        'target-2': [
          {
            avg_latency: 30,
            max_latency: 35,
            min_latency: 25,
            packet_loss: 0.01,
            packet_received: 10,
            packet_sent: 10,
            target_id: 'target-2',
            timestamp: '2026-04-12T10:00:00Z'
          },
          {
            avg_latency: 90,
            max_latency: 95,
            min_latency: 85,
            packet_loss: 0.05,
            packet_received: 7,
            packet_sent: 10,
            target_id: 'target-2',
            timestamp: '2026-04-12T10:01:00Z'
          }
        ]
      }
    })

    render(<ServerCard server={makeServer()} />)

    expect(screen.getByText('80ms')).toBeDefined()
  })

  it('uses warning styling for latency at or above 300ms', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          targets: [
            {
              availability: 1,
              avg_latency: 320,
              max_latency: 330,
              min_latency: 310,
              packet_loss: 0,
              provider: 'intl',
              target_id: 'target-1',
              target_name: 'Tokyo'
            }
          ]
        })
      ]
    })

    render(
      <div className="dark">
        <ServerCard server={makeServer()} />
      </div>
    )

    expect(screen.getByText('320ms').className).toContain('text-amber-600')
  })

  it('uses failure styling when packet loss indicates probe failure', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          targets: [
            {
              availability: 0,
              avg_latency: null,
              max_latency: null,
              min_latency: null,
              packet_loss: 1,
              provider: 'intl',
              target_id: 'target-1',
              target_name: 'Cloudflare'
            }
          ]
        })
      ]
    })

    render(
      <div className="dark">
        <ServerCard server={makeServer()} />
      </div>
    )

    expect(screen.getByText('-').className).toContain('text-red-600')
  })

  it('renders StatusBadge', () => {
    render(<ServerCard server={makeServer({ online: false })} />)
    expect(screen.getByText('offline')).toBeDefined()
  })
})
