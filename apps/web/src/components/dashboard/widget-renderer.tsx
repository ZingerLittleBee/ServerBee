import { Component, memo, type ReactNode, useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { parseConfig } from '@/lib/widget-helpers'
import type {
  AlertListConfig,
  DashboardWidget,
  DiskIoConfig,
  GaugeConfig,
  LineChartConfig,
  MarkdownConfig,
  MultiLineConfig,
  ServerCardsConfig,
  ServerMapConfig,
  ServiceStatusConfig,
  StatNumberConfig,
  TopNConfig,
  TrafficBarConfig,
  UptimeTimelineConfig
} from '@/lib/widget-types'

import { AlertListWidget } from './widgets/alert-list'
import { DiskIoWidget } from './widgets/disk-io'
import { GaugeWidget } from './widgets/gauge'
import { LineChartWidget } from './widgets/line-chart-widget'
import { MarkdownWidget } from './widgets/markdown'
import { MultiLineWidget } from './widgets/multi-line'
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
  resetKey?: string
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

  componentDidUpdate(prevProps: ErrorBoundaryProps) {
    if (this.state.hasError && prevProps.resetKey !== this.props.resetKey) {
      this.setState({ hasError: false })
    }
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

  switch (widget.widget_type) {
    case 'stat-number':
      return <StatNumberWidget config={config as unknown as StatNumberConfig} servers={servers} />
    case 'server-cards':
      return <ServerCardsWidget config={config as unknown as ServerCardsConfig} servers={servers} />
    case 'gauge':
      return <GaugeWidget config={config as unknown as GaugeConfig} servers={servers} />
    case 'line-chart':
      return <LineChartWidget config={config as unknown as LineChartConfig} servers={servers} />
    case 'multi-line':
      return <MultiLineWidget config={config as unknown as MultiLineConfig} servers={servers} />
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
    default:
      return (
        <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
          Unknown widget type: {widget.widget_type}
        </div>
      )
  }
}

const MemoizedWidgetContent = memo(WidgetContent)

export function WidgetRenderer({ widget, servers }: WidgetRendererProps) {
  return (
    <WidgetErrorBoundary fallback={<ErrorFallback />} resetKey={`${widget.id}-${widget.config_json}`}>
      <MemoizedWidgetContent servers={servers} widget={widget} />
    </WidgetErrorBoundary>
  )
}
