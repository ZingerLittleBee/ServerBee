import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const serviceTranslations: Record<string, string> = {
  'actions.addMonitor': '添加监控',
  'aria.deleteMonitor': '删除监控',
  'aria.edit': '编辑监控',
  'aria.triggerCheck': '触发检查',
  'aria.viewDetails': '查看详情',
  'dialog.addDescription': '创建新的服务监控。',
  'dialog.addTitle': '添加监控',
  'dialog.editDescription': '更新服务监控配置。',
  'dialog.editTitle': '编辑监控',
  'empty.createFirst': '创建第一个监控',
  'empty.noMonitors': '暂无服务监控配置。',
  'form.enabled': '启用',
  'form.interval': '间隔（秒）',
  'form.name': '名称',
  'form.target': '目标',
  'form.type': '类型',
  'httpConfig.expectedStatus': '预期状态码（逗号分隔）',
  'httpConfig.keyword': '关键词',
  'httpConfig.keywordExists': '响应中应包含关键词',
  'httpConfig.method': '方法',
  'httpConfig.timeout': '超时（秒）',
  'monitorTypes.dns': 'DNS',
  'monitorTypes.http_keyword': 'HTTP 关键词',
  'monitorTypes.ssl': 'SSL',
  'monitorTypes.tcp': 'TCP',
  'monitorTypes.whois': 'WHOIS',
  namePlaceholder: '我的 SSL 检查',
  'page.title': '服务监控',
  'sslConfig.criticalDays': '严重天数',
  'sslConfig.warningDays': '警告天数',
  'table.actions': '操作',
  'table.enabled': '启用',
  'table.interval': '间隔',
  'table.lastChecked': '最后检查',
  'table.name': '名称',
  'table.status': '状态',
  'table.target': '目标',
  'table.type': '类型',
  'targetPlaceholder.dns': 'example.com',
  'targetPlaceholder.http_keyword': 'https://example.com/health',
  'targetPlaceholder.ssl': 'example.com 或 example.com:8443',
  'targetPlaceholder.tcp': 'example.com:3306',
  'targetPlaceholder.whois': 'example.com',
  'toast.checkTriggered': '检查已触发',
  'toast.createFailed': '创建监控失败',
  'toast.created': '监控已创建',
  'toast.deleteFailed': '删除监控失败',
  'toast.deleted': '监控已删除',
  'toast.toggleFailed': '切换监控状态失败',
  'toast.triggerFailed': '触发检查失败',
  'toast.updateFailed': '更新监控失败',
  'toast.updated': '监控已更新',
  'whoisConfig.criticalDays': '严重天数',
  'whoisConfig.targetHint': 'WHOIS 可填写域名、URL 或 host:port，提交时会自动提取域名部分。',
  'whoisConfig.targetPreview': '将实际查询域名：{{target}}',
  'whoisConfig.unsupportedTldHint': '.app、.dev、.page 等域名通常不提供传统 WHOIS，建议改用 SSL 监控。',
  'whoisConfig.warningDays': '警告天数'
}

const commonTranslations: Record<string, string> = {
  'actions.addMonitor': '添加监控',
  'actions.create': '创建',
  'actions.save': '保存',
  'status.never': '从未'
}

let monitors = [
  {
    config_json: JSON.stringify({ critical_days: 5, warning_days: 21 }),
    consecutive_failures: 0,
    created_at: '2026-04-14T00:00:00Z',
    enabled: true,
    id: 'monitor-1',
    interval: 300,
    last_checked_at: null,
    last_status: null,
    monitor_type: 'whois',
    name: '域名到期监控',
    notification_group_id: null,
    retry_count: 1,
    server_ids_json: null,
    target: 'demo.serverbee.app',
    updated_at: '2026-04-14T00:00:00Z'
  }
]

const mockInvalidateQueries = vi.fn().mockResolvedValue(undefined)
const mockPost = vi.fn()
const mockPut = vi.fn()
const mockDelete = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <a href="/">{children}</a>,
  createFileRoute: () => (config: Record<string, unknown>) => config
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: ({ mutationFn }: { mutationFn?: (input: unknown) => unknown }) => ({
    isPending: false,
    mutate: (input: unknown) => {
      mutationFn?.(input)
    }
  }),
  useQuery: ({ queryKey }: { queryKey: unknown[] }) => {
    if (queryKey[0] === 'service-monitors') {
      return {
        data: monitors,
        isLoading: false
      }
    }

    return {
      data: undefined,
      isLoading: false
    }
  },
  useQueryClient: () => ({
    invalidateQueries: mockInvalidateQueries
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: (namespace?: string) => ({
    t: (key: string, options?: { target?: string }) => {
      const source = namespace === 'common' ? commonTranslations : serviceTranslations
      const template = source[key] ?? key
      return template.replace('{{target}}', options?.target ?? '')
    }
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/components/ui/badge', () => ({
  Badge: ({ children }: { children?: ReactNode }) => <span>{children}</span>
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogDescription: ({ children }: { children?: ReactNode }) => <p>{children}</p>,
  DialogFooter: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />
}))

vi.mock('@/components/ui/select', () => ({
  Select: ({
    items,
    onValueChange,
    value,
    children
  }: {
    children?: ReactNode
    items?: Array<{ label: string; value: string }>
    onValueChange?: (value: string) => void
    value?: string
  }) =>
    items ? (
      <select onChange={(event) => onValueChange?.(event.target.value)} value={value}>
        {items.map((item) => (
          <option key={item.value} value={item.value}>
            {item.label}
          </option>
        ))}
      </select>
    ) : (
      <div>{children}</div>
    ),
  SelectContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectValue: () => <span />
}))

vi.mock('@/components/ui/skeleton', () => ({
  Skeleton: () => <div />
}))

vi.mock('@/components/ui/switch', () => ({
  Switch: ({ checked, onCheckedChange }: { checked?: boolean; onCheckedChange?: (checked: boolean) => void }) => (
    <input checked={checked} onChange={(event) => onCheckedChange?.(event.target.checked)} type="checkbox" />
  )
}))

vi.mock('@/components/ui/table', () => ({
  Table: ({ children }: { children?: ReactNode }) => <table>{children}</table>,
  TableBody: ({ children }: { children?: ReactNode }) => <tbody>{children}</tbody>,
  TableCell: ({ children }: { children?: ReactNode }) => <td>{children}</td>,
  TableHead: ({ children }: { children?: ReactNode }) => <th>{children}</th>,
  TableHeader: ({ children }: { children?: ReactNode }) => <thead>{children}</thead>,
  TableRow: ({ children }: { children?: ReactNode }) => <tr>{children}</tr>
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    delete: mockDelete,
    post: mockPost,
    put: mockPut
  }
}))

const { Route } = await import('./service-monitors')

const ServiceMonitorsPage = Route.component as () => ReactNode

describe('ServiceMonitorsPage', () => {
  beforeEach(() => {
    monitors = [
      {
        config_json: JSON.stringify({ critical_days: 5, warning_days: 21 }),
        consecutive_failures: 0,
        created_at: '2026-04-14T00:00:00Z',
        enabled: true,
        id: 'monitor-1',
        interval: 300,
        last_checked_at: null,
        last_status: null,
        monitor_type: 'whois',
        name: '域名到期监控',
        notification_group_id: null,
        retry_count: 1,
        server_ids_json: null,
        target: 'demo.serverbee.app',
        updated_at: '2026-04-14T00:00:00Z'
      }
    ]
    mockDelete.mockReset()
    mockInvalidateQueries.mockClear()
    mockPost.mockReset()
    mockPut.mockReset()
  })

  it('prefills the edit dialog with the selected monitor data', () => {
    render(<ServiceMonitorsPage />)

    fireEvent.click(screen.getByRole('button', { name: '编辑监控' }))

    expect(screen.getByRole('heading', { name: '编辑监控' })).toBeInTheDocument()
    expect(screen.getByLabelText('名称')).toHaveValue('域名到期监控')
    expect(screen.getByLabelText('目标')).toHaveValue('demo.serverbee.app')
    expect(screen.getByLabelText('间隔（秒）')).toHaveValue(300)
    expect(screen.getByLabelText('警告天数')).toHaveValue(21)
    expect(screen.getByLabelText('严重天数')).toHaveValue(5)
  })

  it('shows whois guidance and normalizes the submitted target host', () => {
    render(<ServiceMonitorsPage />)

    fireEvent.click(screen.getByRole('button', { name: '添加监控' }))

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'whois' } })
    fireEvent.change(screen.getByLabelText('名称'), { target: { value: 'Railway WHOIS' } })
    fireEvent.change(screen.getByLabelText('目标'), { target: { value: 'https://demo.serverbee.app/path' } })

    expect(screen.getByText('WHOIS 可填写域名、URL 或 host:port，提交时会自动提取域名部分。')).toBeInTheDocument()
    expect(screen.getByText('将实际查询域名：demo.serverbee.app')).toBeInTheDocument()
    expect(screen.getByText('.app、.dev、.page 等域名通常不提供传统 WHOIS，建议改用 SSL 监控。')).toBeInTheDocument()

    const form = document.getElementById('monitor-form')
    if (!(form instanceof HTMLFormElement)) {
      throw new Error('monitor form not found')
    }

    fireEvent.submit(form)

    expect(mockPost).toHaveBeenCalledWith('/api/service-monitors', {
      config_json: {},
      enabled: true,
      interval: 300,
      monitor_type: 'whois',
      name: 'Railway WHOIS',
      target: 'demo.serverbee.app'
    })
  })
})
