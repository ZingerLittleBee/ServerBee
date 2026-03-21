import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { WidgetConfigDialog } from './widget-config-dialog'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key
  })
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) =>
    open ? <div data-testid="dialog">{children}</div> : null,
  DialogContent: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-content">{children}</div>,
  DialogFooter: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-footer">{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-header">{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children?: ReactNode }) => <div data-testid="select">{children}</div>,
  SelectContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children, value }: { children?: ReactNode; value: string }) => (
    <option value={value}>{children}</option>
  ),
  SelectTrigger: ({ children }: { children?: ReactNode }) => <div data-testid="select-trigger">{children}</div>,
  SelectValue: ({ placeholder }: { placeholder?: string }) => <span>{placeholder}</span>
}))

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input data-testid="input" {...props} />
}))

vi.mock('@/components/ui/label', () => ({
  Label: ({ children }: { children?: ReactNode }) => <span data-testid="label">{children}</span>
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: (props: Record<string, unknown>) => <input data-testid="checkbox" type="checkbox" {...props} />
}))

vi.mock('@/lib/markdown', () => ({
  renderMarkdown: (content: string) => `<p>${content}</p>`
}))

const mockServers = [
  {
    id: 'srv-1',
    name: 'Server 1',
    online: true,
    cpu: 25,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    swap_used: 0,
    swap_total: 2_000_000_000,
    disk_used: 20_000_000_000,
    disk_total: 50_000_000_000,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 100_000,
    net_out_transfer: 50_000,
    load1: 0.5,
    load5: 0.3,
    load15: 0.2,
    tcp_conn: 42,
    udp_conn: 5,
    process_count: 150,
    uptime: 86_400,
    country_code: 'US',
    os: 'Linux',
    cpu_name: 'Intel Xeon',
    last_active: Date.now(),
    region: null,
    group_id: null
  }
]

const noop = vi.fn()

describe('WidgetConfigDialog', () => {
  it('renders metric select for stat-number widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="stat-number"
      />
    )

    expect(screen.getByText('Configure Widget')).toBeInTheDocument()
    expect(screen.getByText('Metric')).toBeInTheDocument()
    // Stat metrics are available
    expect(screen.getByText('Server Count')).toBeInTheDocument()
    expect(screen.getByText('Average CPU')).toBeInTheDocument()
    expect(screen.getByText('Health')).toBeInTheDocument()
  })

  it('renders server + metric + range selects for line-chart widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="line-chart"
      />
    )

    expect(screen.getByText('Server')).toBeInTheDocument()
    expect(screen.getByText('Metric')).toBeInTheDocument()
    expect(screen.getByText('Time Range')).toBeInTheDocument()
    // Server option
    expect(screen.getByText('Server 1')).toBeInTheDocument()
    // Line metric options
    expect(screen.getByText('CPU')).toBeInTheDocument()
    expect(screen.getByText('Memory')).toBeInTheDocument()
    // Range options
    expect(screen.getByText('1 hour')).toBeInTheDocument()
    expect(screen.getByText('24 hours')).toBeInTheDocument()
  })

  it('renders textarea for markdown widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="markdown"
      />
    )

    expect(screen.getByText('Markdown Content')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Write markdown here...')).toBeInTheDocument()
  })

  it('renders "no config needed" for service-status widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="service-status"
      />
    )

    expect(screen.getByText('No additional configuration needed.')).toBeInTheDocument()
  })

  it('renders "no config needed" for server-map widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="server-map"
      />
    )

    expect(screen.getByText('No additional configuration needed.')).toBeInTheDocument()
  })

  it('renders title input for all widget types', () => {
    render(
      <WidgetConfigDialog onOpenChange={noop} onSubmit={noop} open servers={mockServers as never} widgetType="gauge" />
    )

    expect(screen.getByText('Title (optional)')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Widget title')).toBeInTheDocument()
  })

  it('shows Edit Widget title when editing existing widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widget={{
          id: 'w-1',
          dashboard_id: 'dash-1',
          widget_type: 'stat-number',
          title: 'My Stat',
          config_json: '{"metric":"avg_cpu"}',
          grid_x: 0,
          grid_y: 0,
          grid_w: 2,
          grid_h: 2,
          sort_order: 0,
          created_at: '2026-03-20T00:00:00Z'
        }}
        widgetType="stat-number"
      />
    )

    expect(screen.getByText('Edit Widget')).toBeInTheDocument()
  })

  it('does not render when closed', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open={false}
        servers={mockServers as never}
        widgetType="stat-number"
      />
    )

    expect(screen.queryByText('Configure Widget')).not.toBeInTheDocument()
  })
})
