export type WidgetCategory = 'Real-time' | 'Charts' | 'Status'

export type SizingStrategy =
  | { kind: 'free' }
  | { kind: 'fixed' }
  | { kind: 'aspect-square'; tiers: readonly number[] }
  | { kind: 'content-height' }

export interface WidgetTypeDefinition {
  category: WidgetCategory
  defaultH: number
  defaultW: number
  id: string
  label: string
  maxH?: number
  maxW?: number
  minH: number
  minW: number
  sizing: SizingStrategy
}

export const WIDGET_TYPES = [
  {
    id: 'stat-number',
    label: 'Stat Number',
    category: 'Real-time',
    defaultW: 2,
    defaultH: 1,
    minW: 2,
    minH: 1,
    maxW: 2,
    maxH: 1,
    sizing: { kind: 'fixed' }
  },
  {
    id: 'metric-card',
    label: 'Metric Card',
    category: 'Real-time',
    defaultW: 4,
    defaultH: 4,
    minW: 3,
    minH: 3,
    maxW: 6,
    maxH: 6,
    sizing: { kind: 'free' }
  },
  {
    id: 'server-cards',
    label: 'Server Cards',
    category: 'Real-time',
    defaultW: 12,
    defaultH: 6,
    minW: 4,
    minH: 3,
    sizing: { kind: 'free' }
  },
  {
    id: 'gauge',
    label: 'Gauge',
    category: 'Real-time',
    defaultW: 2,
    defaultH: 2,
    minW: 2,
    minH: 2,
    maxW: 6,
    maxH: 6,
    sizing: { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
  },
  {
    id: 'line-chart',
    label: 'Line Chart',
    category: 'Charts',
    defaultW: 6,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'multi-line',
    label: 'Multi Line',
    category: 'Charts',
    defaultW: 8,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'top-n',
    label: 'Top N',
    category: 'Real-time',
    defaultW: 4,
    defaultH: 2,
    minW: 3,
    minH: 2,
    maxW: 6,
    maxH: 8,
    sizing: { kind: 'content-height' }
  },
  {
    id: 'alert-list',
    label: 'Alert List',
    category: 'Status',
    defaultW: 4,
    defaultH: 4,
    minW: 3,
    minH: 2,
    maxW: 8,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'service-status',
    label: 'Service Status',
    category: 'Status',
    defaultW: 6,
    defaultH: 3,
    minW: 3,
    minH: 2,
    maxW: 12,
    maxH: 6,
    sizing: { kind: 'free' }
  },
  {
    id: 'traffic-bar',
    label: 'Traffic Bar',
    category: 'Charts',
    defaultW: 6,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'disk-io',
    label: 'Disk I/O',
    category: 'Charts',
    defaultW: 6,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'server-map',
    label: 'Server Map',
    category: 'Status',
    defaultW: 8,
    defaultH: 5,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'markdown',
    label: 'Markdown',
    category: 'Status',
    defaultW: 4,
    defaultH: 3,
    minW: 2,
    minH: 2,
    sizing: { kind: 'free' }
  },
  {
    id: 'uptime-timeline',
    label: 'Uptime Timeline',
    category: 'Status',
    defaultW: 8,
    defaultH: 3,
    minW: 4,
    minH: 2,
    maxW: 12,
    maxH: 6,
    sizing: { kind: 'free' }
  },
  {
    id: 'network-latency',
    label: 'Network Latency',
    category: 'Charts',
    defaultW: 6,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'network-quality',
    label: 'Network Quality',
    category: 'Real-time',
    defaultW: 4,
    defaultH: 4,
    minW: 3,
    minH: 3,
    maxW: 8,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'network-overview',
    label: 'Network Overview',
    category: 'Status',
    defaultW: 8,
    defaultH: 5,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  }
] as const satisfies readonly WidgetTypeDefinition[]

export type WidgetTypeId = (typeof WIDGET_TYPES)[number]['id']

// Per-type widget configurations stored as JSON in config_json

export interface StatNumberConfig {
  label?: string
  metric: string
  server_id: string
  unit?: string
}

export type MetricCardMetric = 'cpu' | 'memory' | 'network' | 'disk_io'

export interface MetricCardConfig {
  label?: string
  metric: MetricCardMetric
  server_id: string
}

export interface ServerCardsConfig {
  columns?: number
  server_ids?: string[]
}

export interface GaugeConfig {
  label?: string
  max?: number
  metric: string
  server_id: string
}

export interface LineChartConfig {
  hours?: number
  interval?: string
  metric: string
  server_id: string
}

export interface MultiLineConfig {
  hours?: number
  interval?: string
  metric: string
  server_ids: string[]
}

export interface TopNConfig {
  count?: number
  metric: string
  sort?: 'asc' | 'desc'
}

export interface AlertListConfig {
  max_items?: number
  server_ids?: string[]
}

export interface ServiceStatusConfig {
  monitor_ids?: string[]
}

export interface TrafficBarConfig {
  hours?: number
  server_id: string
}

export interface DiskIoConfig {
  hours?: number
  interval?: string
  server_id: string
}

export interface ServerMapConfig {
  server_ids?: string[]
}

export interface MarkdownConfig {
  content: string
}

export interface UptimeTimelineConfig {
  days?: number
  server_ids: string[]
}

export interface NetworkLatencyConfig {
  hours?: number // 0 means realtime
  server_id: string
}

export interface NetworkQualityConfig {
  server_id: string
}

export interface NetworkOverviewConfig {
  server_ids?: string[]
}

export type WidgetConfig =
  | StatNumberConfig
  | MetricCardConfig
  | ServerCardsConfig
  | GaugeConfig
  | LineChartConfig
  | MultiLineConfig
  | TopNConfig
  | AlertListConfig
  | ServiceStatusConfig
  | TrafficBarConfig
  | DiskIoConfig
  | ServerMapConfig
  | MarkdownConfig
  | UptimeTimelineConfig
  | NetworkLatencyConfig
  | NetworkQualityConfig
  | NetworkOverviewConfig

// API response types

export interface DashboardWidget {
  config_json: string
  created_at: string
  dashboard_id: string
  grid_h: number
  grid_w: number
  grid_x: number
  grid_y: number
  id: string
  module_id?: string | null
  sort_order: number
  title: string | null
  widget_type: string
}

export interface Dashboard {
  created_at: string
  id: string
  is_default: boolean
  name: string
  sort_order: number
  updated_at: string
}

export interface DashboardWithWidgets extends Dashboard {
  widgets: DashboardWidget[]
}
