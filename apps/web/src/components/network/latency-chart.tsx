import { lazy, Suspense } from 'react'
import { Skeleton } from '@/components/ui/skeleton'
import type { NetworkProbeRecord } from '@/lib/network-types'

export interface TargetInfo {
  color: string
  id: string
  name: string
  visible: boolean
}

export interface LatencyChartProps {
  // Dashboard widgets already provide card chrome and a sized flex container.
  embedded?: boolean
  hours?: number
  isRealtime?: boolean
  records: NetworkProbeRecord[]
  targets: TargetInfo[]
}

const LazyLatencyChartContent = lazy(() =>
  import('./latency-chart-content').then((module) => ({
    default: module.LatencyChartContent
  }))
)

export function LatencyChart(props: LatencyChartProps) {
  return (
    <Suspense
      fallback={<Skeleton className={props.embedded ? 'h-full min-h-0 w-full' : 'h-[332px] w-full rounded-lg'} />}
    >
      <LazyLatencyChartContent {...props} />
    </Suspense>
  )
}
