import { render } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

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

    return { data: [] }
  }
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@/components/server/agent-version-section', () => ({
  AgentVersionSection: () => <div>agent-version</div>
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
  UptimeTimeline: () => <div>uptime</div>
}))

vi.mock('@/hooks/use-api', () => ({
  useServer: () => ({
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
  }),
  useServerRecords: () => ({ data: [] }),
  useUptimeDaily: () => ({ data: [] })
}))

vi.mock('@/hooks/use-realtime-metrics', () => ({
  useRealtimeMetrics: () => []
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

const { ServerDetailPage } = await import('./$id')

describe('ServerDetailPage', () => {
  it('keeps bottom padding on the page container', () => {
    const { container } = render(<ServerDetailPage />)

    expect(container.firstElementChild).toHaveClass('pb-6')
  })
})
