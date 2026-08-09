import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import type { ServerCostOverview } from '@/lib/api-schema'
import type { NetworkServerSummary } from '@/lib/network-types'
import { CostFootnote } from './cost-footnote'
import { ServerCard } from './server-card'

const REGEX_COST_PER_HOUR = /0\.01\/h/
const REGEX_COST_PER_MONTH = /7\.30\/mo/
const REGEX_OPACITY_UTILITY = /\bopacity-/

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    i18n: { language: 'en' },
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

const mockNetworkRealtime = vi.fn()
vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: (...args: unknown[]) => mockNetworkRealtime(...args)
}))

type CardProps = Parameters<typeof ServerCard>[0]

function renderCard(server: CardProps['server'], extra: Omit<CardProps, 'server'> = {}) {
  return render(
    <TooltipProvider>
      <ServerCard server={server} {...extra} />
    </TooltipProvider>
  )
}

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
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
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
    tags: undefined,
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
    mockNetworkRealtime.mockReturnValue({ data: {} })
  })

  it('renders server name', () => {
    renderCard(makeServer())
    expect(screen.getByText('test-server')).toBeDefined()
  })

  it('renders four ring charts with CPU, Memory, Disk, Traffic labels', () => {
    renderCard(makeServer())
    expect(screen.getByText('col_cpu')).toBeDefined()
    expect(screen.getByText('col_memory')).toBeDefined()
    expect(screen.getByText('col_disk')).toBeDefined()
    expect(screen.getByText('card_traffic_quota')).toBeDefined()
  })

  it('renders footnote secondary metrics', () => {
    renderCard(makeServer())
    expect(screen.getByText('col_uptime')).toBeDefined()
    expect(screen.getByText('card_swap')).toBeDefined()
    expect(screen.getByText('card_proc_conn_label')).toBeDefined()
  })

  it('renders compact cost footnote when cost overview is available', () => {
    renderCard(makeServer(), {
      costEntry: {
        configured: true,
        cost_per_hour: 0.01,
        cost_per_month_equivalent: 7.3,
        currency: 'USD',
        name: 'test-server',
        server_id: 'srv-1',
        advisories: []
      } satisfies ServerCostOverview
    })

    expect(screen.getByText(REGEX_COST_PER_HOUR)).toBeDefined()
    expect(screen.getByText(REGEX_COST_PER_MONTH)).toBeDefined()
  })

  it('renders compact unconfigured cost footnote labels', () => {
    const missingPrice = {
      configured: false,
      invalid_reason: 'missing_price',
      name: 'test-server',
      server_id: 'srv-1',
      advisories: []
    } satisfies ServerCostOverview
    const missingCycle = {
      configured: false,
      invalid_reason: 'missing_billing_cycle',
      name: 'test-server',
      server_id: 'srv-1',
      advisories: []
    } satisfies ServerCostOverview
    const invalidPrice = {
      configured: false,
      invalid_reason: 'invalid_price',
      name: 'test-server',
      server_id: 'srv-1',
      advisories: []
    } satisfies ServerCostOverview

    const { rerender } = render(<CostFootnote entry={missingPrice} />)
    expect(screen.getByText('cost_not_set')).toBeDefined()

    rerender(<CostFootnote entry={missingCycle} />)
    expect(screen.getByText('cost_price_only')).toBeDefined()

    rerender(<CostFootnote entry={invalidPrice} />)
    expect(screen.getByText('cost_invalid')).toBeDefined()
  })

  it('renders network and disk I/O rates with load trend', () => {
    renderCard(makeServer())
    expect(screen.getByText('card_net_in_speed')).toBeDefined()
    expect(screen.getByText('card_net_out_speed')).toBeDefined()
    expect(screen.getByText('card_disk_read')).toBeDefined()
    expect(screen.getByText('card_disk_write')).toBeDefined()
    expect(screen.getByText('card_load_trend')).toBeDefined()
  })

  it('reserves the network quality section as a placeholder when no probe data exists', () => {
    renderCard(makeServer())
    expect(screen.getByLabelText('card_network_quality')).toBeDefined()
    expect(screen.getByText('card_latency')).toBeDefined()
    expect(screen.getByText('card_packet_loss')).toBeDefined()
  })

  it('renders latency and loss square grids when network data is present', () => {
    renderCard(makeServer(), {
      networkSummary: makeSummary({
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
          }
        ]
      })
    })

    expect(screen.getByText('card_latency')).toBeDefined()
    expect(screen.getByText('card_packet_loss')).toBeDefined()
  })

  it('renders tag chips when server.tags is non-empty', () => {
    renderCard(makeServer({ tags: ['CN2 GIA', 'AS9929'] }))
    expect(screen.getByText('CN2 GIA')).toBeDefined()
    expect(screen.getByText('AS9929')).toBeDefined()
  })

  it('does not render tag chip container when tags is empty', () => {
    renderCard(makeServer({ tags: [] }))
    expect(screen.queryByText('CN2 GIA')).toBeNull()
  })

  it('renders StatusBadge', () => {
    renderCard(makeServer({ online: false }))
    expect(screen.getByText('common:status.offline')).toBeDefined()
  })

  it('marks offline cards with a muted surface and desaturates metrics only', () => {
    const { container } = renderCard(makeServer({ online: false }))
    const card = container.firstElementChild as HTMLElement
    const metrics = card.querySelector('.grayscale')

    // Surface + destructive ring make offline scannable next to live cards.
    expect(card.className).toContain('bg-muted/70')
    expect(card.className).toContain('ring-destructive/35')
    // Grayscale is scoped to metrics so the StatusBadge keeps its red pill.
    expect(card.className).not.toContain('grayscale')
    expect(metrics).not.toBeNull()
    expect(card.className).not.toMatch(REGEX_OPACITY_UTILITY)
    // A translucent scrim stacked over the body pushed it to 1.82:1.
    expect(container.innerHTML).not.toContain('bg-background/')
  })

  it('gives the truncated name a title so the full value stays reachable', () => {
    renderCard(makeServer({ name: 'a-very-long-server-name-that-will-be-clipped' }))
    const heading = screen.getByRole('heading', { level: 3 })

    expect(heading).toHaveClass('truncate')
    expect(heading).toHaveAttribute('title', 'a-very-long-server-name-that-will-be-clipped')
  })

  it('hides the decorative OS emoji from the accessible link name', () => {
    const { container } = renderCard(makeServer({ os: 'Ubuntu 22.04' }))
    const emoji = container.querySelector('span[title="Ubuntu 22.04"]')

    expect(emoji).not.toBeNull()
    expect(emoji).toHaveAttribute('aria-hidden', 'true')
  })

  it('lets the card shrink below the ideal 320px card width', () => {
    const { container } = renderCard(makeServer())
    expect((container.firstElementChild as HTMLElement).className).toContain('min-w-0')
  })

  it('stretches to fill the grid cell so online and offline heights match', () => {
    const { container } = renderCard(makeServer({ online: false }))
    expect((container.firstElementChild as HTMLElement).className).toContain('h-full')
  })
})
