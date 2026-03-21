import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { WidgetRenderer } from './widget-renderer'

// Mock all widget components to simple stubs
vi.mock('./widgets/stat-number', () => ({
  StatNumberWidget: () => <div data-testid="widget-stat-number">stat-number</div>
}))
vi.mock('./widgets/server-cards', () => ({
  ServerCardsWidget: () => <div data-testid="widget-server-cards">server-cards</div>
}))
vi.mock('./widgets/gauge', () => ({
  GaugeWidget: () => <div data-testid="widget-gauge">gauge</div>
}))
vi.mock('./widgets/line-chart-widget', () => ({
  LineChartWidget: () => <div data-testid="widget-line-chart">line-chart</div>
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
  ServerMapWidget: () => <div data-testid="widget-server-map">server-map</div>
}))
vi.mock('./widgets/markdown', () => ({
  MarkdownWidget: () => <div data-testid="widget-markdown">markdown</div>
}))
vi.mock('./widgets/uptime-timeline-widget', () => ({
  UptimeTimelineWidget: () => <div data-testid="widget-uptime-timeline">uptime-timeline</div>
}))

function makeWidget(widgetType: string): DashboardWidget {
  return {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: widgetType,
    title: null,
    config_json: '{}',
    grid_x: 0,
    grid_y: 0,
    grid_w: 4,
    grid_h: 3,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
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

describe('WidgetRenderer', () => {
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
})
