import type { WidgetManifest, WidgetModule } from '@serverbee/widget-sdk'
import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { DashboardWidget } from '@/lib/widget-types'
import { registryActions, useWidgetRegistry } from '@/widgets-runtime/registry'
import { WidgetRenderer } from './widget-renderer'

const { renderCounts } = vi.hoisted(() => ({
  renderCounts: {
    gauge: 0,
    'line-chart': 0,
    'server-map': 0
  }
}))

// Mock all widget components to simple stubs
vi.mock('./widgets/stat-number', () => ({
  StatNumberWidget: () => <div data-testid="widget-stat-number">stat-number</div>
}))
vi.mock('./widgets/server-cards', () => ({
  ServerCardsWidget: () => <div data-testid="widget-server-cards">server-cards</div>
}))
vi.mock('./widgets/gauge', () => ({
  GaugeWidget: () => {
    renderCounts.gauge += 1
    return <div data-testid="widget-gauge">gauge</div>
  }
}))
vi.mock('./widgets/line-chart-widget', () => ({
  LineChartWidget: () => {
    renderCounts['line-chart'] += 1
    return <div data-testid="widget-line-chart">line-chart</div>
  }
}))
vi.mock('./widgets/multi-line', () => ({
  MultiLineWidget: () => <div data-testid="widget-multi-line">multi-line</div>
}))
vi.mock('./widgets/top-n', () => ({
  TopNWidget: () => <div data-testid="widget-top-n">top-n</div>
}))
vi.mock('./widgets/alert-list', () => ({
  AlertListWidget: () => <div data-testid="widget-alert-list">alert-list</div>
}))
vi.mock('./widgets/service-status', () => ({
  ServiceStatusWidget: () => <div data-testid="widget-service-status">service-status</div>
}))
vi.mock('./widgets/traffic-bar', () => ({
  TrafficBarWidget: () => <div data-testid="widget-traffic-bar">traffic-bar</div>
}))
vi.mock('./widgets/disk-io', () => ({
  DiskIoWidget: () => <div data-testid="widget-disk-io">disk-io</div>
}))
vi.mock('./widgets/server-map', () => ({
  ServerMapWidget: () => {
    renderCounts['server-map'] += 1
    return <div data-testid="widget-server-map">server-map</div>
  }
}))
vi.mock('./widgets/markdown', () => ({
  MarkdownWidget: () => <div data-testid="widget-markdown">markdown</div>
}))
vi.mock('./widgets/uptime-timeline-widget', () => ({
  UptimeTimelineWidget: () => <div data-testid="widget-uptime-timeline">uptime-timeline</div>
}))

function makeWidget(
  widgetType: string,
  config: Record<string, unknown> = {},
  extra: Partial<DashboardWidget> = {}
): DashboardWidget {
  return {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: widgetType,
    title: null,
    config_json: JSON.stringify(config),
    grid_x: 0,
    grid_y: 0,
    grid_w: 4,
    grid_h: 3,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z',
    ...extra
  }
}

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'Server 1',
    online: true,
    country_code: null,
    cpu: 10,
    cpu_name: null,
    cpu_cores: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 100,
    disk_used: 20,
    disk_write_bytes_per_sec: 0,
    group_id: null,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 100,
    mem_used: 50,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

const WIDGET_TYPES = [
  'stat-number',
  'server-cards',
  'gauge',
  'line-chart',
  'multi-line',
  'top-n',
  'alert-list',
  'service-status',
  'traffic-bar',
  'disk-io',
  'server-map',
  'markdown',
  'uptime-timeline'
] as const

const NOT_INSTALLED_RE = /not installed/i

describe('WidgetRenderer', () => {
  beforeEach(() => {
    renderCounts.gauge = 0
    renderCounts['line-chart'] = 0
    renderCounts['server-map'] = 0
  })

  for (const widgetType of WIDGET_TYPES) {
    it(`renders ${widgetType} without crashing`, () => {
      render(<WidgetRenderer servers={[]} widget={makeWidget(widgetType)} />)
      expect(screen.getByTestId(`widget-${widgetType}`)).toBeInTheDocument()
    })
  }

  it('renders fallback for unknown widget type', () => {
    render(<WidgetRenderer servers={[]} widget={makeWidget('unknown-type')} />)
    expect(screen.getByText('Unknown widget type: unknown-type')).toBeInTheDocument()
  })

  it('does not rerender a single-server chart when an unrelated server updates', () => {
    const widget = makeWidget('line-chart', { metric: 'memory', server_id: 's1' })
    const server = makeServer({ id: 's1', name: 'Primary', mem_total: 100, mem_used: 50 })
    const unrelated = makeServer({ id: 's2', name: 'Unrelated', cpu: 20 })

    const { rerender } = render(<WidgetRenderer servers={[server, unrelated]} widget={widget} />)

    expect(renderCounts['line-chart']).toBe(1)

    rerender(<WidgetRenderer servers={[server, { ...unrelated, cpu: 90, last_active: 1 }]} widget={widget} />)

    expect(renderCounts['line-chart']).toBe(1)
  })

  it('does not rerender a historical chart when only live fields change', () => {
    const widget = makeWidget('line-chart', { metric: 'cpu', server_id: 's1' })
    const server = makeServer({ id: 's1', name: 'Primary', cpu: 20 })

    const { rerender } = render(<WidgetRenderer servers={[server]} widget={widget} />)

    expect(renderCounts['line-chart']).toBe(1)

    rerender(<WidgetRenderer servers={[{ ...server, cpu: 90, last_active: 1 }]} widget={widget} />)

    expect(renderCounts['line-chart']).toBe(1)
  })

  it('does not rerender the server map when only live fields change', () => {
    const widget = makeWidget('server-map')
    const server = makeServer({ id: 's1', name: 'Primary', country_code: 'US', cpu: 20 })

    const { rerender } = render(<WidgetRenderer servers={[server]} widget={widget} />)

    expect(renderCounts['server-map']).toBe(1)

    rerender(<WidgetRenderer servers={[{ ...server, cpu: 90, last_active: 1 }]} widget={widget} />)

    expect(renderCounts['server-map']).toBe(1)
  })

  it('rerenders a realtime gauge when its live metric changes', () => {
    const widget = makeWidget('gauge', { metric: 'cpu', server_id: 's1' })
    const server = makeServer({ id: 's1', name: 'Primary', cpu: 20 })

    const { rerender } = render(<WidgetRenderer servers={[server]} widget={widget} />)

    expect(renderCounts.gauge).toBe(1)

    rerender(<WidgetRenderer servers={[{ ...server, cpu: 90 }]} widget={widget} />)

    expect(renderCounts.gauge).toBe(2)
  })

  describe('module widgets', () => {
    beforeEach(() => {
      useWidgetRegistry.setState({ modules: new Map(), failures: new Map() })
    })

    const fakeManifest: WidgetManifest = {
      id: 'com.test.fake',
      version: '1.0.0',
      name: 'Fake',
      category: 'Real-time',
      sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
      sdkVersion: '^0.1.0'
    }

    function makeFakeModule(component: WidgetModule['component']): WidgetModule {
      return {
        __brand: 'WidgetModule',
        configSchema: { parse: (v: unknown) => v } as unknown as WidgetModule['configSchema'],
        component,
        actions: []
      }
    }

    it('renders the registered module component', () => {
      registryActions.register(
        'com.test.fake',
        makeFakeModule(() => <div data-testid="fake-module">hello from module</div>),
        fakeManifest
      )
      render(<WidgetRenderer servers={[]} widget={makeWidget('module', {}, { module_id: 'com.test.fake' })} />)
      expect(screen.getByTestId('fake-module')).toBeInTheDocument()
    })

    it('shows a placeholder when the referenced module is not installed', () => {
      render(<WidgetRenderer servers={[]} widget={makeWidget('module', {}, { module_id: 'com.test.missing' })} />)
      expect(screen.getByText(NOT_INSTALLED_RE)).toBeInTheDocument()
    })
  })
})
