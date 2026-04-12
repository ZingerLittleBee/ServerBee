import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

const mockNavigate = vi.fn()
const lastProbePattern = /最后探测:/

const translationMap: Record<string, string> = {
  all_targets: '所有目标',
  avg_latency: '平均延迟',
  availability: '可用性',
  back_to_overview: '返回总览',
  by_provider: '按运营商',
  cancel: '取消',
  deselect_all: '取消全选',
  export_csv: '导出 CSV',
  global_settings: '全局设置',
  last_probe: '最后探测',
  manage_targets: '管理目标',
  no_targets: '未配置探测目标',
  packet_loss: '丢包率',
  probe_type_http: 'HTTP 探测',
  probe_type_icmp: 'ICMP 探测',
  probe_type_tcp: 'TCP 探测',
  realtime: '实时',
  save: '保存',
  select_all: '全选',
  server_not_found: '未找到服务器',
  server_os: '操作系统',
  server_region: '地区',
  targets: '目标数',
  traceroute: '路由追踪'
}

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <a href="/network">{children}</a>,
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => mockNavigate,
    useParams: () => ({ serverId: 'server-1' }),
    useSearch: () => ({ range: 'realtime' })
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => translationMap[key] ?? options?.defaultValue ?? key
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({
    user: { role: 'admin' }
  })
}))

vi.mock('@/hooks/use-api', () => ({
  useServer: () => ({
    data: {
      id: 'server-1',
      ipv4: '1.2.3.4',
      ipv6: null,
      os: 'Ubuntu 24.04',
      region: '成都'
    },
    isLoading: false
  })
}))

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkAnomalies: () => ({ data: [] }),
  useNetworkRecords: () => ({ data: [] }),
  useNetworkServerSummary: () => ({
    data: {
      anomaly_count: 0,
      last_probe_at: '2026-04-12T12:34:00Z',
      latency_sparkline: [],
      loss_sparkline: [],
      online: true,
      server_id: 'server-1',
      server_name: '成都节点',
      targets: [
        {
          availability: 0.99,
          avg_latency: 20,
          max_latency: 30,
          min_latency: 10,
          packet_loss: 0,
          provider: 'ct',
          target_id: 'target-1',
          target_name: '中国电信'
        }
      ]
    },
    isLoading: false
  }),
  useNetworkTargets: () => ({
    data: [
      {
        created_at: null,
        id: 'target-1',
        location: '成都',
        name: '中国电信',
        probe_type: 'tcp',
        provider: '电信',
        source: null,
        source_name: null,
        target: 'example.com:443',
        updated_at: null
      }
    ]
  }),
  useSetServerTargets: () => ({
    isPending: false,
    mutate: vi.fn()
  }),
  useStartTraceroute: () => ({
    isPending: false,
    mutate: vi.fn()
  }),
  useTracerouteResult: () => ({ data: null })
}))

vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: () => ({ data: {} })
}))

vi.mock('@/components/network/anomaly-table', () => ({
  AnomalyTable: () => <div data-testid="anomaly-table" />
}))

vi.mock('@/components/network/latency-chart', () => ({
  LatencyChart: () => <div data-testid="latency-chart" />
}))

vi.mock('@/components/network/target-card', () => ({
  TargetCard: ({ target }: { target: { target_name: string } }) => <div>{target.target_name}</div>
}))

vi.mock('@/components/server/status-badge', () => ({
  StatusBadge: () => <div>online</div>
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: (props: Record<string, unknown>) => <input aria-label="checkbox" type="checkbox" {...props} />
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  DialogClose: ({ children }: { children?: ReactNode }) => <button type="button">{children}</button>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

const { Route } = await import('./$serverId')

describe('NetworkDetailPage', () => {
  it('renders translated network detail labels and probe types in the manage targets dialog', () => {
    render(<Route.component />)

    expect(screen.getByText(lastProbePattern)).toBeInTheDocument()
    expect(screen.getByText('地区: 成都')).toBeInTheDocument()
    expect(screen.getByText('操作系统: Ubuntu 24.04')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '管理目标' }))

    expect(screen.getByText('TCP 探测')).toBeInTheDocument()
  })
})
