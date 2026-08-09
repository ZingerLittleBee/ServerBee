import { Component, memo, type ReactNode, useMemo } from 'react'
import type { ServerMetrics } from '@/lib/server-catalog'
import { parseConfig } from '@/lib/widget-helpers'
import type {
  AlertListConfig,
  DashboardWidget,
  DiskIoConfig,
  GaugeConfig,
  LineChartConfig,
  MarkdownConfig,
  MetricCardConfig,
  MultiLineConfig,
  NetworkLatencyConfig,
  NetworkOverviewConfig,
  NetworkQualityConfig,
  ServerCardsConfig,
  ServerMapConfig,
  ServiceStatusConfig,
  StatNumberConfig,
  TopNConfig,
  TrafficBarConfig,
  UptimeTimelineConfig
} from '@/lib/widget-types'
import { ModuleWidgetHost } from './module-widget-host'
import { areWidgetServerDependenciesEqual } from './widget-render-dependencies'
import { AlertListWidget } from './widgets/alert-list'
import { DiskIoWidget } from './widgets/disk-io'
import { GaugeWidget } from './widgets/gauge'
import { LineChartWidget } from './widgets/line-chart-widget'
import { MarkdownWidget } from './widgets/markdown'
import { MetricCardWidget } from './widgets/metric-card'
import { MultiLineWidget } from './widgets/multi-line'
import { NetworkLatencyWidget } from './widgets/network-latency-widget'
import { NetworkOverviewWidget } from './widgets/network-overview-widget'
import { NetworkQualityWidget } from './widgets/network-quality'
import { ServerCardsWidget } from './widgets/server-cards'
import { ServerMapWidget } from './widgets/server-map'
import { ServiceStatusWidget } from './widgets/service-status'
import { StatNumberWidget } from './widgets/stat-number'
import { TopNWidget } from './widgets/top-n'
import { TrafficBarWidget } from './widgets/traffic-bar'
import { UptimeTimelineWidget } from './widgets/uptime-timeline-widget'

interface WidgetRendererProps {
  servers: ServerMetrics[]
  widget: DashboardWidget
}

interface ErrorBoundaryProps {
  children: ReactNode
  fallback: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
}

// biome-ignore lint/style/useReactFunctionComponents: ErrorBoundary requires class component (no function-based API in React)
class WidgetErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false }
  }

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true }
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback
    }
    return this.props.children
  }
}

function ErrorFallback() {
  return (
    <div className="flex h-full items-center justify-center rounded-lg border border-destructive/30 bg-card p-4 text-destructive text-sm">
      Widget failed to render
    </div>
  )
}

function WidgetContent({ widget, servers }: WidgetRendererProps) {
  const config = useMemo(() => parseConfig<Record<string, unknown>>(widget.config_json), [widget.config_json])

  if (widget.widget_type === 'module') {
    return <ModuleWidgetHost servers={servers} widget={widget} />
  }

  switch (widget.widget_type) {
    case 'stat-number':
      return <StatNumberWidget config={config as unknown as StatNumberConfig} servers={servers} title={widget.title} />
    case 'metric-card':
      return <MetricCardWidget config={config as unknown as MetricCardConfig} servers={servers} />
    case 'server-cards':
      return <ServerCardsWidget config={config as unknown as ServerCardsConfig} servers={servers} />
    case 'gauge':
      return <GaugeWidget config={config as unknown as GaugeConfig} servers={servers} />
    case 'line-chart':
      return <LineChartWidget config={config as unknown as LineChartConfig} servers={servers} title={widget.title} />
    case 'multi-line':
      return <MultiLineWidget config={config as unknown as MultiLineConfig} servers={servers} title={widget.title} />
    case 'top-n':
      return <TopNWidget config={config as unknown as TopNConfig} servers={servers} />
    case 'alert-list':
      return <AlertListWidget config={config as unknown as AlertListConfig} servers={servers} />
    case 'service-status':
      return <ServiceStatusWidget config={config as unknown as ServiceStatusConfig} />
    case 'traffic-bar':
      return <TrafficBarWidget config={config as unknown as TrafficBarConfig} servers={servers} />
    case 'disk-io':
      return <DiskIoWidget config={config as unknown as DiskIoConfig} servers={servers} />
    case 'server-map':
      return <ServerMapWidget config={config as unknown as ServerMapConfig} servers={servers} />
    case 'markdown':
      return <MarkdownWidget config={config as unknown as MarkdownConfig} />
    case 'uptime-timeline':
      return <UptimeTimelineWidget config={config as unknown as UptimeTimelineConfig} servers={servers} />
    case 'network-quality':
      return <NetworkQualityWidget config={config as unknown as NetworkQualityConfig} servers={servers} />
    case 'network-latency':
      return <NetworkLatencyWidget config={config as unknown as NetworkLatencyConfig} servers={servers} />
    case 'network-overview':
      return <NetworkOverviewWidget config={config as unknown as NetworkOverviewConfig} servers={servers} />
    default:
      return (
        <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
          Unknown widget type: {widget.widget_type}
        </div>
      )
  }
}

/**
 * `is_static` only toggles grid drag/resize. Including it in remount keys or
 * memo deps remounts charts every lock/unlock click. Strip it for content
 * identity while keeping real config edits as remount/reset triggers.
 */
function contentConfigFingerprint(configJson: string): string {
  try {
    const config = JSON.parse(configJson) as Record<string, unknown>
    if (!Object.hasOwn(config, 'is_static')) {
      return configJson
    }
    const { is_static: _isStatic, ...rest } = config
    return JSON.stringify(rest)
  } catch {
    return configJson
  }
}

function areWidgetContentPropsEqual(prev: WidgetRendererProps, next: WidgetRendererProps): boolean {
  if (
    prev.widget.id !== next.widget.id ||
    prev.widget.widget_type !== next.widget.widget_type ||
    prev.widget.title !== next.widget.title ||
    contentConfigFingerprint(prev.widget.config_json) !== contentConfigFingerprint(next.widget.config_json)
  ) {
    return false
  }

  return areWidgetServerDependenciesEqual(next.widget, prev.servers, next.servers)
}

const MemoizedWidgetContent = memo(WidgetContent, areWidgetContentPropsEqual)

export function WidgetRenderer({ widget, servers }: WidgetRendererProps) {
  return (
    <WidgetErrorBoundary
      fallback={<ErrorFallback />}
      key={`${widget.id}-${widget.widget_type}-${widget.title ?? ''}-${contentConfigFingerprint(widget.config_json)}`}
    >
      <MemoizedWidgetContent servers={servers} widget={widget} />
    </WidgetErrorBoundary>
  )
}
