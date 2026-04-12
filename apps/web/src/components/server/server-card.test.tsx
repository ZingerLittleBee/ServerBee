import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { ServerCard } from './server-card'

const NULL_BAR_COLOR = 'var(--color-muted)'

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

const mockNetworkOverview = vi.fn()
vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: (...args: unknown[]) => mockNetworkOverview(...args)
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

  it('renders network quality from the matching overview summary', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          server_id: 'other-server',
          latency_sparkline: [999],
          loss_sparkline: [0.99]
        }),
        makeSummary({
          latency_sparkline: [30, null, 60],
          loss_sparkline: [0.01, null, 0.02]
        })
      ]
    })
    render(<ServerCard server={makeServer()} />)

    expect(screen.getByLabelText('Latency trend')).toBeDefined()
    expect(screen.getByLabelText('Packet loss trend')).toBeDefined()
    expect(screen.getByText('45ms')).toBeDefined()
    expect(screen.getByText('1.5%')).toBeDefined()

    const latencyBars = screen.getByLabelText('Latency trend').querySelectorAll('[data-testid="uptime-bar-item"]')
    const lossBars = screen.getByLabelText('Packet loss trend').querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(latencyBars.length).toBe(30)
    expect(lossBars.length).toBe(30)
  })

  it('does not render network quality when the matching summary has no non-null sparkline data', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          server_id: 'other-server',
          latency_sparkline: [25],
          loss_sparkline: [0.02]
        }),
        makeSummary({
          latency_sparkline: [null, null],
          loss_sparkline: [null, null]
        })
      ]
    })
    render(<ServerCard server={makeServer()} />)
    expect(screen.queryByLabelText('Latency trend')).toBeNull()
  })

  it('uses placeholder styling when loss summary data is entirely null', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          latency_sparkline: [40, 80],
          loss_sparkline: [null, null]
        })
      ]
    })
    render(
      <div className="dark">
        <ServerCard server={makeServer()} />
      </div>
    )

    const packetLossRow = screen.getByText('card_packet_loss').parentElement
    const packetLossValue = packetLossRow?.querySelector('.font-medium')
    const lossBars = screen.getByLabelText('Packet loss trend').querySelectorAll('[data-testid="uptime-bar-item"]')

    expect(packetLossValue?.textContent).toBe('-')
    expect(packetLossValue?.className).toContain('text-muted-foreground')
    expect((lossBars[0] as HTMLElement).style.backgroundColor).toBe(NULL_BAR_COLOR)
  })

  it('uses placeholder styling when latency summary data is entirely null', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          latency_sparkline: [null, null],
          loss_sparkline: [0.02, 0.03]
        })
      ]
    })
    render(
      <div className="dark">
        <ServerCard server={makeServer()} />
      </div>
    )

    const latencyRow = screen.getByText('card_latency').parentElement
    const latencyValue = latencyRow?.querySelector('.font-medium')
    const latencyBars = screen.getByLabelText('Latency trend').querySelectorAll('[data-testid="uptime-bar-item"]')

    expect(latencyValue?.textContent).toBe('-')
    expect(latencyValue?.className).toContain('text-muted-foreground')
    expect((latencyBars[0] as HTMLElement).style.backgroundColor).toBe(NULL_BAR_COLOR)
  })

  it('renders StatusBadge', () => {
    render(<ServerCard server={makeServer({ online: false })} />)
    expect(screen.getByText('offline')).toBeDefined()
  })
})
