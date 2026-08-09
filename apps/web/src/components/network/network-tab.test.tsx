import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

const lastProbePattern = /最后探测:/
const traceroutePattern = /路由追踪/
const exportCsvPattern = /导出 CSV/
const manageTargetsPattern = /管理目标/

const translationMap: Record<string, string> = {
  all_targets: '所有目标',
  avg_latency: '平均延迟',
  availability: '可用性',
  builtin: '内置',
  by_provider: '按运营商',
  cancel: '取消',
  deselect_all: '取消全选',
  export_csv: '导出 CSV',
  last_probe: '最后探测',
  location_shanghai: '上海',
  manage_targets: '管理目标',
  no_targets: '未配置探测目标',
  packet_loss: '丢包率',
  probe_type_http: 'HTTP 探测',
  probe_type_icmp: 'ICMP 探测',
  probe_type_tcp: 'TCP 探测',
  provider_short_telecom: '电信',
  realtime: '实时',
  save: '保存',
  select_all: '全选',
  targets: '目标数',
  traceroute: '路由追踪'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    i18n: {
      language: 'zh-CN',
      resolvedLanguage: 'zh-CN'
    },
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

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkAnomalies: () => ({ data: [] }),
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
          target_name: 'Shanghai Telecom'
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
        location: 'Shanghai',
        name: 'Shanghai Telecom',
        probe_type: 'tcp',
        provider: 'Telecom',
        source: 'builtin',
        source_name: null,
        target: 'example.com:443',
        updated_at: null
      }
    ]
  }),
  useSetServerTargets: () => ({
    isPending: false,
    mutate: vi.fn()
  })
}))

vi.mock('@/hooks/use-network-chart-records', () => ({
  useNetworkChartRecords: () => ({ records: [] })
}))

vi.mock('@/components/network/latency-chart', () => ({
  LatencyChart: () => <div data-testid="latency-chart" />
}))

vi.mock('@/components/network/traceroute-dialog', () => ({
  TracerouteDialog: ({ open }: { open: boolean }) => (open ? <div data-testid="traceroute-dialog" /> : null)
}))

vi.mock('@/components/network/anomaly-table', () => ({
  AnomalyTable: () => <div data-testid="anomaly-table" />
}))

vi.mock('@/components/network/target-card', () => ({
  TargetCard: ({ displayName }: { displayName: string }) => <div>{displayName}</div>
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

const { NetworkTab } = await import('./network-tab')

describe('NetworkTab', () => {
  it('renders the toolbar with the last probe time and localized target names', () => {
    render(<NetworkTab serverId="server-1" />)

    expect(screen.getByText(lastProbePattern)).toBeInTheDocument()
    expect(screen.getAllByText('上海电信').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByRole('button', { name: traceroutePattern })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: exportCsvPattern })).toBeInTheDocument()
  })

  it('renders translated probe types in the manage targets dialog', () => {
    render(<NetworkTab serverId="server-1" />)

    fireEvent.click(screen.getByRole('button', { name: manageTargetsPattern }))

    expect(screen.getByText('TCP 探测')).toBeInTheDocument()
    expect(screen.getAllByText('上海电信').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('电信')).toBeInTheDocument()
    expect(screen.getByText('上海')).toBeInTheDocument()
  })

  it('opens the traceroute dialog from the toolbar', () => {
    render(<NetworkTab serverId="server-1" />)

    fireEvent.click(screen.getByRole('button', { name: traceroutePattern }))

    expect(screen.getByTestId('traceroute-dialog')).toBeInTheDocument()
  })
})
