import type { LucideIcon } from 'lucide-react'
import { Cpu, HardDriveDownload, MemoryStick, Network } from 'lucide-react'
import { formatSpeed } from '@/lib/utils'
import type { MetricCardMetric } from '@/lib/widget-types'

export type DeltaUnit = 'pp' | 'percent'
export type DeltaTone = 'semantic' | 'neutral'

export interface MetricCardSpec {
  accent: string
  deltaTone: DeltaTone
  deltaUnit: DeltaUnit
  formatValue: (n: number) => string
  icon: LucideIcon
  labelKey: string
}

const formatPercent = (n: number) => `${n.toFixed(1)}%`

export const METRIC_CARD_SPECS: Record<MetricCardMetric, MetricCardSpec> = {
  cpu: {
    icon: Cpu,
    accent: '--chart-4',
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.cpu'
  },
  memory: {
    icon: MemoryStick,
    accent: '--chart-3',
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.memory'
  },
  network: {
    icon: Network,
    accent: '--chart-1',
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.network'
  },
  disk_io: {
    icon: HardDriveDownload,
    accent: '--chart-2',
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.diskIo'
  }
}
