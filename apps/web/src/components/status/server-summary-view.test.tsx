import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { PublicServerSummary, PublicStatusConfig } from '@/lib/api-schema'
import { ServerSummaryCard } from './server-summary-card'
import { ServerSummaryRow } from './server-summary-row'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    i18n: { language: 'en' },
    t: (key: string) => key
  })
}))

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...props }: { children?: React.ReactNode; [k: string]: unknown }) => (
    <a
      data-testid="status-server-link"
      href={`/status/server/${props.params && (props.params as { serverId: string }).serverId}`}
    >
      {children}
    </a>
  )
}))

const thresholds: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'> = {
  uptime_red_threshold: 95,
  uptime_yellow_threshold: 100
}

const REGEX_PERCENT_LABEL = /%$/

function makeServer(overrides: Partial<PublicServerSummary> = {}): PublicServerSummary {
  return {
    country_code: 'US',
    group_name: 'edge',
    id: 'srv-1',
    in_maintenance: false,
    metrics: {
      cpu: 72,
      disk_read_bytes_per_sec: 2_100_000,
      disk_total: 53_687_091_200,
      disk_used: 21_474_836_480,
      disk_write_bytes_per_sec: 512_000,
      load_1: 0.72,
      load_5: 0.65,
      load_15: 0.58,
      mem_total: 8_589_934_592,
      mem_used: 4_294_967_296,
      net_in_speed: 12_900_000,
      net_in_transfer: 1_099_511_627_776,
      net_out_speed: 4_300_000,
      net_out_transfer: 549_755_813_888,
      process_count: 142,
      swap_total: 2_147_483_648,
      swap_used: 536_870_912,
      tcp_conn: 38,
      udp_conn: 12,
      uptime: 1_987_200
    },
    name: 'status-server',
    online: true,
    os: 'Ubuntu 22.04',
    public_remark: null,
    region: 'Los Angeles',
    uptime_daily: [],
    uptime_percent: 99.8,
    ...overrides
  }
}

describe('ServerSummaryCard', () => {
  it('uses the same dense card shell as the /servers grid view', () => {
    const { container } = render(<ServerSummaryCard clickable server={makeServer()} />)

    const card = container.querySelector('[data-slot="status-server-card"]')
    expect(card?.className).toContain('min-w-[320px]')
    expect(card?.className).toContain('max-w-[480px]')
    expect(card?.className).toContain('p-3')
    expect(screen.getAllByRole('img', { name: REGEX_PERCENT_LABEL })).toHaveLength(4)
    expect(screen.getByText('card_net_in_speed')).toBeDefined()
    expect(screen.getByText('card_net_out_speed')).toBeDefined()
    expect(screen.getByText('card_disk_read')).toBeDefined()
    expect(screen.getByText('card_disk_write')).toBeDefined()
    expect(screen.getByText('card_swap')).toBeDefined()
    expect(screen.getByText('card_proc_conn_label')).toBeDefined()
  })
})

describe('ServerSummaryRow', () => {
  it('uses the /servers table-density row styling for the public list view', () => {
    const { container } = render(
      <table>
        <tbody>
          <ServerSummaryRow clickable server={makeServer()} thresholds={thresholds} />
        </tbody>
      </table>
    )

    const row = container.querySelector('[data-slot="status-server-row"]')
    expect(row?.className).toContain('h-[72px]')
    expect(screen.getByTestId('status-server-link')).toHaveAttribute('href', '/status/server/srv-1')
    expect(container.textContent ?? '').toContain('72%')
    expect(container.textContent ?? '').toContain('12.3 MB/s')
    expect(container.textContent ?? '').toContain('2.0 MB/s')
    expect(container.textContent ?? '').toContain('142 / 38 / 12')
  })
})
