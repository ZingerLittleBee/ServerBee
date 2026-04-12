import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockNavigate = vi.fn()

const translationMaps = {
  en: {
    add_target: 'Add Target',
    builtin: 'Built-in',
    cancel: 'Cancel',
    confirm_delete_target: 'Are you sure you want to delete this target?',
    custom: 'Custom',
    default_targets: 'Default Targets',
    default_targets_desc: 'Probe targets assigned to new servers by default',
    delete_target: 'Delete Target',
    delete_target_aria: 'Delete {{name}}',
    edit_target: 'Edit Target',
    edit_target_aria: 'Edit {{name}}',
    global_settings: 'Global Settings',
    location_beijing: 'Beijing',
    no_targets: 'No probe targets configured',
    probe_interval: 'Probe Interval',
    probe_interval_desc: 'Seconds between probe rounds (30-600)',
    probe_type_http: 'HTTP Probe',
    probe_type_icmp: 'ICMP Probe',
    probe_type_tcp: 'TCP Probe',
    provider_short_telecom: 'Telecom',
    save: 'Save',
    settings_title: 'Network Probe Settings',
    target_actions: 'Actions',
    target_address: 'Address',
    target_location: 'Location',
    target_management: 'Target Management',
    target_name: 'Name',
    target_provider: 'Provider',
    target_status: 'Status',
    target_type: 'Probe Type'
  },
  zh: {
    add_target: '添加目标',
    builtin: '内置',
    cancel: '取消',
    confirm_delete_target: '确定要删除此目标吗？',
    custom: '自定义',
    default_targets: '默认目标',
    default_targets_desc: '新服务器自动分配的探测目标',
    delete_target: '删除目标',
    delete_target_aria: '删除 {{name}}',
    edit_target: '编辑目标',
    edit_target_aria: '编辑 {{name}}',
    global_settings: '全局设置',
    location_beijing: '北京',
    no_targets: '未配置探测目标',
    probe_interval: '探测间隔',
    probe_interval_desc: '探测轮次间隔秒数 (30-600)',
    probe_type_http: 'HTTP 探测',
    probe_type_icmp: 'ICMP 探测',
    probe_type_tcp: 'TCP 探测',
    provider_short_telecom: '电信',
    save: '保存',
    settings_title: '网络探测设置',
    target_actions: '操作',
    target_address: '地址',
    target_location: '地区',
    target_management: '目标管理',
    target_name: '名称',
    target_provider: '运营商',
    target_status: '状态',
    target_type: '探测类型'
  }
} satisfies Record<'en' | 'zh', Record<string, string>>

let currentLanguage: 'en' | 'zh' = 'zh'

const stableT = (key: string, options?: { defaultValue?: string; name?: string }) => {
  const value = translationMaps[currentLanguage][key] ?? options?.defaultValue ?? key
  return value.replace('{{name}}', options?.name ?? '')
}

const networkTargets = [
  {
    created_at: null,
    id: 'target-1',
    location: 'Beijing',
    name: 'Beijing Telecom',
    probe_type: 'icmp',
    provider: 'Telecom',
    source: 'builtin',
    source_name: null,
    target: '1.1.1.1',
    updated_at: null
  },
  {
    created_at: null,
    id: 'target-2',
    location: '香港',
    name: '自定义 TCP 目标',
    probe_type: 'tcp',
    provider: '',
    source: null,
    source_name: null,
    target: 'example.com:443',
    updated_at: null
  }
]

const networkSetting = {
  default_target_ids: ['target-1'],
  interval: 60,
  packet_count: 10
}

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => mockNavigate,
    useSearch: () => ({ tab: 'targets' })
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    i18n: {
      language: currentLanguage,
      resolvedLanguage: currentLanguage
    },
    t: stableT
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/hooks/use-network-api', () => ({
  useCreateTarget: () => ({ isPending: false, mutate: vi.fn() }),
  useDeleteTarget: () => ({ isPending: false, mutate: vi.fn() }),
  useNetworkSetting: () => ({
    data: networkSetting
  }),
  useNetworkTargets: () => ({
    data: networkTargets,
    isLoading: false
  }),
  useUpdateNetworkSetting: () => ({ isPending: false, mutate: vi.fn() }),
  useUpdateTarget: () => ({ isPending: false, mutate: vi.fn() })
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button {...props}>{children}</button>
  )
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: (props: Record<string, unknown>) => <input aria-label="checkbox" type="checkbox" {...props} />
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />
}))

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectValue: () => <div />
}))

vi.mock('@/components/ui/skeleton', () => ({
  Skeleton: () => <div />
}))

vi.mock('@/components/ui/tabs', () => ({
  Tabs: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsList: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  TabsTrigger: ({ children }: { children?: ReactNode }) => <button type="button">{children}</button>
}))

const { NetworkProbeSettingsPage } = await import('./network-probes')

describe('NetworkProbeSettingsPage', () => {
  beforeEach(() => {
    currentLanguage = 'zh'
    mockNavigate.mockReset()
  })

  it('renders translated probe target labels in the targets table and controls', () => {
    render(<NetworkProbeSettingsPage />)

    expect(screen.getByRole('columnheader', { name: '状态' })).toBeInTheDocument()
    expect(screen.getByRole('columnheader', { name: '操作' })).toBeInTheDocument()
    expect(screen.getAllByText('北京电信').length).toBeGreaterThan(0)
    expect(screen.queryByText('Beijing Telecom')).not.toBeInTheDocument()
    expect(screen.getAllByText('ICMP 探测').length).toBeGreaterThan(0)
    expect(screen.getByText('内置')).toBeInTheDocument()
    expect(screen.getByText('自定义')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '编辑 自定义 TCP 目标' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '删除 自定义 TCP 目标' })).toBeInTheDocument()
  })

  it('updates translated column headers after a language change', () => {
    currentLanguage = 'en'
    const { rerender } = render(<NetworkProbeSettingsPage />)

    expect(screen.getByRole('columnheader', { name: 'Name' })).toBeInTheDocument()
    expect(screen.queryByRole('columnheader', { name: '名称' })).not.toBeInTheDocument()

    currentLanguage = 'zh'
    rerender(<NetworkProbeSettingsPage />)

    expect(screen.getByRole('columnheader', { name: '名称' })).toBeInTheDocument()
    expect(screen.queryByRole('columnheader', { name: 'Name' })).not.toBeInTheDocument()
  })
})
