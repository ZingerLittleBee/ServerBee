import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerCostInsights } from '@/lib/api-schema'

const REGEX_DETAIL_EXPIRED = /detail_expired/

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <a href="/">{children}</a>,
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => vi.fn(),
    useParams: () => ({ id: 'server-1' }),
    useSearch: () => ({ range: 'realtime' })
  })
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: ({ queryKey }: { queryKey: unknown[] }) => {
    if (queryKey[0] === 'servers') {
      return { data: [] }
    }

    if (queryKey[0] === 'agent' && queryKey[1] === 'latest-version') {
      return { data: { version: '1.3.0' } }
    }

    return { data: [] }
  }
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key
  })
}))

vi.mock('@/components/server/agent-version-section', () => ({
  AgentVersionSection: ({ latestVersion }: { latestVersion?: string | null }) => (
    <div data-testid="agent-version-section">{latestVersion ?? 'no-latest-version'}</div>
  )
}))

vi.mock('@/components/server/recovery-merge-dialog', () => ({
  RecoveryMergeDialog: () => <div data-testid="recovery-dialog" />
}))

vi.mock('@/components/server/capabilities-dialog', () => ({
  CapabilitiesDialog: () => <div>capabilities</div>
}))

vi.mock('@/components/server/disk-io-chart', () => ({
  DiskIoChart: () => <div>disk-io</div>
}))

vi.mock('@/components/server/metrics-chart', () => ({
  MetricsChart: () => <div>metrics-chart</div>
}))

vi.mock('@/components/server/server-edit-dialog', () => ({
  ServerEditDialog: () => null
}))

vi.mock('@/components/server/status-badge', () => ({
  StatusBadge: () => <div>online</div>
}))

vi.mock('@/components/server/traffic-card', () => ({
  TrafficCard: () => <div>traffic-card</div>
}))

vi.mock('@/components/server/traffic-progress', () => ({
  TrafficProgress: () => <div>traffic-progress</div>
}))

vi.mock('@/components/server/traffic-tab', () => ({
  TrafficTab: () => <div>traffic-tab</div>
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode }) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/skeleton', () => ({
  Skeleton: (props: Record<string, unknown>) => <div {...props} />
}))

vi.mock('@/components/ui/tabs', () => ({
  Tabs: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsList: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsTrigger: ({ children }: { children?: ReactNode }) => <button type="button">{children}</button>
}))

vi.mock('@/components/uptime/uptime-timeline', () => ({
  UptimeTimeline: () => <div data-testid="uptime-timeline">uptime</div>
}))

const mockUseServer = vi.fn()
const mockUseServerRecords = vi.fn()
const mockUseUptimeDaily = vi.fn()
const mockUseCostInsights = vi.fn()

vi.mock('@/hooks/use-api', () => ({
  useServer: (serverId: string) => mockUseServer(serverId),
  useServerRecords: (...args: unknown[]) => mockUseServerRecords(...args),
  useUptimeDaily: (serverId: string) => mockUseUptimeDaily(serverId)
}))

vi.mock('@/hooks/use-cost', () => ({
  useCostInsights: (serverId: string) => mockUseCostInsights(serverId)
}))

vi.mock('@/hooks/use-realtime-metrics', () => ({
  useRealtimeMetrics: () => []
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({
    user: { role: 'admin' }
  })
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: vi.fn()
  }
}))

vi.mock('@/lib/capabilities', () => ({
  CAP_DOCKER: 1,
  CAP_FILE: 2,
  CAP_TERMINAL: 4,
  getEffectiveCapabilityEnabled: () => false
}))

vi.mock('@/lib/disk-io', () => ({
  buildMergedDiskIoSeries: () => [],
  buildPerDiskIoSeries: () => []
}))

vi.mock('@/lib/utils', () => ({
  cn: (...classes: Array<string | false | null | undefined>) => classes.filter(Boolean).join(' '),
  countryCodeToFlag: () => 'US',
  formatBytes: (value: number) => `${value}`
}))

vi.mock('@/lib/widget-helpers', () => ({
  computeAggregateUptime: () => null
}))

vi.mock('@/stores/upgrade-jobs-store', () => ({
  useUpgradeJobsStore: () => undefined
}))

vi.mock('@/stores/recovery-jobs-store', () => ({
  useRecoveryJobsStore: (selector: (state: { hydrated: boolean; jobs: Map<string, unknown> }) => unknown) =>
    selector({
      hydrated: true,
      jobs: new Map()
    })
}))

const { ServerDetailPage } = await import('./$id')

describe('ServerDetailPage', () => {
  beforeEach(() => {
    mockUseServer.mockReturnValue({
      data: {
        agent_version: '1.0.0',
        billing_cycle: null,
        capabilities: 0,
        country_code: 'US',
        cpu_arch: 'x86_64',
        cpu_cores: 4,
        cpu_name: 'Test CPU',
        disk_total: 1,
        effective_capabilities: 0,
        id: 'server-1',
        ip_v4: null,
        ip_v6: null,
        ipv4: '127.0.0.1',
        ipv6: null,
        kernel_version: '6.0.0',
        mem_total: 1,
        name: 'test-server',
        os: 'Ubuntu',
        price: null,
        protocol_version: 1,
        region: 'test-region',
        traffic_limit: null
      },
      isLoading: false
    })
    mockUseServerRecords.mockReturnValue({ data: [] })
    mockUseUptimeDaily.mockReturnValue({ data: [] })
    mockUseCostInsights.mockReturnValue({ data: undefined })
  })

  it('keeps bottom padding on the page container', () => {
    const { container } = render(<ServerDetailPage />)

    expect(container.firstElementChild).toHaveClass('pb-6')
  })

  it('passes latest agent version into the version section', () => {
    const { container } = render(<ServerDetailPage />)

    expect(container).toHaveTextContent('1.3.0')
  })

  it('places the upgrade card in its own full-width header row', () => {
    mockUseUptimeDaily.mockReturnValue({
      data: [{ date: '2026-04-14', total: 100, up: 100 }]
    })

    render(<ServerDetailPage />)

    const agentMeta = screen.getByText('detail_agent_label')
    const upgradeCard = screen.getByTestId('agent-version-section')
    const editButton = screen.getByText('detail_edit')
    const headerGrid = upgradeCard.parentElement?.parentElement

    expect(upgradeCard.parentElement).toHaveClass('sm:col-span-2')
    expect(headerGrid?.children[0]).toContainElement(agentMeta)
    expect(headerGrid?.children[1]).toContainElement(upgradeCard)
    expect(headerGrid?.children[2]).toContainElement(editButton)
  })

  it('shows recovery action for offline server when admin', () => {
    render(<ServerDetailPage />)

    expect(screen.getByText('Recover Agent')).toBeInTheDocument()
  })

  it('shows cost insights on the billing summary', () => {
    mockUseServer.mockReturnValue({
      data: {
        agent_version: '1.0.0',
        billing_cycle: 'monthly',
        capabilities: 0,
        country_code: 'US',
        cpu_arch: 'x86_64',
        cpu_cores: 4,
        cpu_name: 'Test CPU',
        currency: 'USD',
        disk_total: 1,
        effective_capabilities: 0,
        expired_at: '2020-01-01T00:00:00Z',
        id: 'server-1',
        ip_v4: null,
        ip_v6: null,
        ipv4: '127.0.0.1',
        ipv6: null,
        kernel_version: '6.0.0',
        mem_total: 1,
        name: 'test-server',
        os: 'Ubuntu',
        price: 5,
        protocol_version: 1,
        region: 'test-region',
        traffic_limit: null
      },
      isLoading: false
    })
    mockUseCostInsights.mockReturnValue({
      isError: true,
      data: {
        billing_cycle: 'monthly',
        configured: true,
        cost_per_day: 0.16,
        cost_per_hour: 0.0068,
        cost_per_month_equivalent: 5,
        cost_per_second: 0.000_001_9,
        currency: 'USD',
        cycle_burn_percent: 54.2,
        cycle_cost_elapsed: 2.71,
        cycle_cost_remaining: 2.29,
        cycle_days: 30,
        cycle_end: null,
        cycle_start: null,
        days_elapsed: 16,
        days_remaining: 14,
        invalid_reason: null,
        price: 5,
        resource_value: {},
        server_id: 'server-1',
        value_score: {
          confidence: 'high',
          grade: 'good',
          reasons: ['healthy_uptime'],
          score: 82
        }
      } satisfies ServerCostInsights
    })

    render(<ServerDetailPage />)

    expect(screen.getByText('cost_value_score')).toBeInTheDocument()
    expect(screen.getByText('82')).toBeInTheDocument()
    expect(screen.getByText(REGEX_DETAIL_EXPIRED)).toBeInTheDocument()
  })

  it('keeps expiration status when cost config is incomplete', () => {
    mockUseServer.mockReturnValue({
      data: {
        agent_version: '1.0.0',
        billing_cycle: null,
        capabilities: 0,
        country_code: 'US',
        cpu_arch: 'x86_64',
        cpu_cores: 4,
        cpu_name: 'Test CPU',
        currency: 'USD',
        disk_total: 1,
        effective_capabilities: 0,
        expired_at: '2020-01-01T00:00:00Z',
        id: 'server-1',
        ip_v4: null,
        ip_v6: null,
        ipv4: '127.0.0.1',
        ipv6: null,
        kernel_version: '6.0.0',
        mem_total: 1,
        name: 'test-server',
        os: 'Ubuntu',
        price: 5,
        protocol_version: 1,
        region: 'test-region',
        traffic_limit: null
      },
      isLoading: false
    })
    mockUseCostInsights.mockReturnValue({
      data: {
        billing_cycle: null,
        configured: false,
        currency: 'USD',
        invalid_reason: 'missing_billing_cycle',
        price: 5,
        server_id: 'server-1'
      } satisfies ServerCostInsights
    })

    render(<ServerDetailPage />)

    expect(screen.getByText(REGEX_DETAIL_EXPIRED)).toBeInTheDocument()
    expect(screen.getByText('cost_invalid_missing_billing_cycle')).toBeInTheDocument()
  })
})
