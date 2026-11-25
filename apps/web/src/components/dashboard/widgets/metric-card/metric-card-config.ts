import { formatSpeed } from '@/lib/utils'
import type { MetricCardMetric } from '@/lib/widget-types'

export type DeltaUnit = 'pp' | 'percent'
export type DeltaTone = 'semantic' | 'neutral'

export interface MetricCardSpec {
  deltaTone: DeltaTone
  deltaUnit: DeltaUnit
  formatValue: (n: number) => string
  labelKey: string
}

/** Every metric shares one accent; status colors are reserved for the delta. */
export const METRIC_CARD_ACCENT = 'var(--chart-1)'

const formatPercent = (n: number) => `${n.toFixed(1)}%`

export const METRIC_CARD_SPECS: Record<MetricCardMetric, MetricCardSpec> = {
  cpu: {
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.cpu'
  },
  memory: {
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.memory'
  },
  network: {
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.network'
  },
  disk_io: {
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.diskIo'
  }
}
