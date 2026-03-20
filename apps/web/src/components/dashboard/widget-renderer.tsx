import { Component, type ReactNode } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
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
  TrafficBarConfig
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

interface WidgetRendererProps {
  servers: ServerMetrics[]
  widget: DashboardWidget
}

// Error boundary to catch widget render errors
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

function parseConfig<T>(configJson: string): T {
  try {
    return JSON.parse(configJson) as T
  } catch {
    return {} as T
  }
}

function ErrorFallback() {
  return (
    <div className="flex h-full items-center justify-center rounded-lg border border-destructive/30 bg-card p-4 text-destructive text-sm">
      Widget failed to render
    </div>
  )
}

function renderWidgetContent(widget: DashboardWidget, servers: ServerMetrics[]): ReactNode {
  switch (widget.widget_type) {
    case 'stat-number':
      return <StatNumberWidget config={parseConfig<StatNumberConfig>(widget.config_json)} servers={servers} />
    case 'server-cards':
      return <ServerCardsWidget config={parseConfig<ServerCardsConfig>(widget.config_json)} servers={servers} />
    case 'gauge':
      return <GaugeWidget config={parseConfig<GaugeConfig>(widget.config_json)} servers={servers} />
    case 'line-chart':
      return <LineChartWidget config={parseConfig<LineChartConfig>(widget.config_json)} servers={servers} />
    case 'multi-line':
      return <MultiLineWidget config={parseConfig<MultiLineConfig>(widget.config_json)} servers={servers} />
    case 'top-n':
      return <TopNWidget config={parseConfig<TopNConfig>(widget.config_json)} servers={servers} />
    case 'alert-list':
      return <AlertListWidget config={parseConfig<AlertListConfig>(widget.config_json)} servers={servers} />
    case 'service-status':
      return <ServiceStatusWidget config={parseConfig<ServiceStatusConfig>(widget.config_json)} />
    case 'traffic-bar':
      return <TrafficBarWidget config={parseConfig<TrafficBarConfig>(widget.config_json)} servers={servers} />
    case 'disk-io':
      return <DiskIoWidget config={parseConfig<DiskIoConfig>(widget.config_json)} servers={servers} />
    case 'server-map':
      return <ServerMapWidget config={parseConfig<ServerMapConfig>(widget.config_json)} servers={servers} />
    case 'markdown':
      return <MarkdownWidget config={parseConfig<MarkdownConfig>(widget.config_json)} />
    default:
      return (
        <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
          Unknown widget type: {widget.widget_type}
        </div>
      )
  }
}

export function WidgetRenderer({ widget, servers }: WidgetRendererProps) {
  return <WidgetErrorBoundary fallback={<ErrorFallback />}>{renderWidgetContent(widget, servers)}</WidgetErrorBoundary>
}
