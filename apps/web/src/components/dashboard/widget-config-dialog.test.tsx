import { defineWidget, type WidgetManifest, z } from '@serverbee/widget-sdk'
import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { registryActions } from '@/widgets-runtime/registry'
import { WidgetConfigDialog } from './widget-config-dialog'

const translations: Record<string, string> = {
  'dialogs.widgetConfig.configureTitle': 'Configure Widget',
  'dialogs.widgetConfig.editTitle': 'Edit Widget',
  'dialogs.widgetConfig.labels.titleOptional': 'Title (optional)',
  'dialogs.widgetConfig.placeholders.widgetTitle': 'Widget title',
  'dialogs.widgetConfig.messages.noConfigNeeded': 'No additional configuration needed.',
  'widgets.common.labels.server': 'Server',
  'widgets.common.labels.servers': 'Servers',
  'widgets.common.labels.metric': 'Metric',
  'widgets.common.labels.timeRange': 'Time Range',
  'widgets.common.labels.days': 'Days',
  'widgets.common.labels.markdownContent': 'Markdown Content',
  'widgets.common.placeholders.writeMarkdown': 'Write markdown here...',
  'common.metrics.serverCount': 'Server Count',
  'common.metrics.avgCpu': 'Average CPU',
  'common.metrics.avgMemory': 'Average Memory',
  'common.metrics.health': 'Health',
  'common.metrics.cpu': 'CPU',
  'common.metrics.memory': 'Memory',
  'widgets.common.placeholders.selectServer': 'Select server',
  'widgets.common.empty.noServers': 'No servers',
  'common.timeRange.realtime': 'Realtime',
  'common.timeRange.1hour': '1 hour',
  'common.timeRange.6hours': '6 hours',
  'common.timeRange.7days': '7 days',
  'common.timeRange.24hours': '24 hours',
  'common.timeRange.30days': '30 days',
  'common.timeRange.60days': '60 days',
  'common.timeRange.90days': '90 days',
  module_not_installed: 'Widget module not installed',
  module_not_installed_id: 'Widget module "{{id}}" not installed',
  module_config_no_fields: 'This module exposes no configurable fields.',
  save: 'Save',
  add_widget: 'Add'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => translations[key] ?? fallback ?? key
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
  Checkbox: ({
    checked,
    onCheckedChange,
    ...props
  }: {
    checked?: boolean
    onCheckedChange?: (checked: boolean) => void
  } & Record<string, unknown>) => (
    <input
      checked={checked}
      data-testid="checkbox"
      onChange={() => onCheckedChange?.(!checked)}
      type="checkbox"
      {...props}
    />
  )
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
const NOT_INSTALLED_RE = /not installed/i

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

  it('renders server multi-select and days selector for uptime-timeline widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="uptime-timeline"
      />
    )

    expect(screen.getByText('Servers')).toBeInTheDocument()
    expect(screen.getByText('Days')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
    expect(screen.getByText('30 days')).toBeInTheDocument()
    expect(screen.getByText('60 days')).toBeInTheDocument()
    expect(screen.getByText('90 days')).toBeInTheDocument()
  })

  it('renders server + range (with realtime) for network-latency widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-latency"
      />
    )

    expect(screen.getByText('Server')).toBeInTheDocument()
    expect(screen.getByText('Time Range')).toBeInTheDocument()
    expect(screen.getByText('Realtime')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })

  it('renders a server select for network-quality widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-quality"
      />
    )

    expect(screen.getByText('Server')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })

  it('renders a server multi-select for network-overview widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-overview"
      />
    )

    expect(screen.getByText('Servers')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })

  describe('module widgets', () => {
    const moduleId = 'com.test.cfg-dialog'
    const fakeManifest: WidgetManifest = {
      id: moduleId,
      version: '1.0.0',
      name: 'Test',
      category: 'Real-time',
      sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
      sdkVersion: '^0.1.0'
    }

    afterEach(() => {
      registryActions.unregister(moduleId)
    })

    it('renders the SDK config form fields for a registered module widget', () => {
      const module = defineWidget({
        configSchema: z.object({
          label: z.string().describe('Label')
        }),
        component: () => <div />
      })
      registryActions.register(moduleId, module, fakeManifest)

      const onSubmit = vi.fn()
      render(
        <WidgetConfigDialog
          onOpenChange={noop}
          onSubmit={onSubmit}
          open
          servers={mockServers as never}
          widget={{
            id: 'w-mod',
            dashboard_id: 'd-1',
            widget_type: 'module',
            module_id: moduleId,
            title: null,
            config_json: '{"label":"hi"}',
            grid_x: 0,
            grid_y: 0,
            grid_w: 2,
            grid_h: 2,
            sort_order: 0,
            created_at: '2026-03-20T00:00:00Z'
          }}
          widgetType="module"
        />
      )

      // The renderer surfaces the field label
      expect(screen.getByText('Label')).toBeInTheDocument()
      // Existing value is loaded into the input
      const labelInput = screen.getByDisplayValue('hi') as HTMLInputElement
      expect(labelInput).toBeInTheDocument()

      // Type a new value and save
      fireEvent.change(labelInput, { target: { value: 'world' } })
      fireEvent.click(screen.getByText('Save'))
      expect(onSubmit).toHaveBeenCalledTimes(1)
      const [, configJson] = onSubmit.mock.calls[0]
      expect(JSON.parse(configJson)).toMatchObject({ label: 'world' })
    })

    it('shows a placeholder when the module is not installed and disables save', () => {
      const onSubmit = vi.fn()
      render(
        <WidgetConfigDialog
          onOpenChange={noop}
          onSubmit={onSubmit}
          open
          servers={mockServers as never}
          widget={{
            id: 'w-mod',
            dashboard_id: 'd-1',
            widget_type: 'module',
            module_id: 'com.does.not.exist',
            title: null,
            config_json: '{}',
            grid_x: 0,
            grid_y: 0,
            grid_w: 2,
            grid_h: 2,
            sort_order: 0,
            created_at: '2026-03-20T00:00:00Z'
          }}
          widgetType="module"
        />
      )

      expect(screen.getByText(NOT_INSTALLED_RE)).toBeInTheDocument()
      const saveButton = screen.getByText('Save') as HTMLButtonElement
      expect(saveButton).toBeDisabled()
    })

    it('shows a "no configurable fields" message for modules with empty configSchema', () => {
      const module = defineWidget({
        configSchema: z.object({}),
        component: () => <div />
      })
      registryActions.register(moduleId, module, fakeManifest)

      render(
        <WidgetConfigDialog
          onOpenChange={noop}
          onSubmit={noop}
          open
          servers={mockServers as never}
          widget={{
            id: 'w-mod',
            dashboard_id: 'd-1',
            widget_type: 'module',
            module_id: moduleId,
            title: null,
            config_json: '{}',
            grid_x: 0,
            grid_y: 0,
            grid_w: 2,
            grid_h: 2,
            sort_order: 0,
            created_at: '2026-03-20T00:00:00Z'
          }}
          widgetType="module"
        />
      )

      expect(screen.getByText('This module exposes no configurable fields.')).toBeInTheDocument()
    })
  })
})
