import { SparklinePlot } from '@/components/charts/sparkline-plot'
import type { MetricSeriesPoint } from '@/hooks/use-metric-series'

interface MetricCardSparklineProps {
  accent: string
  points: MetricSeriesPoint[]
}

export function MetricCardSparkline({ points, accent }: MetricCardSparklineProps) {
  if (points.length < 2) {
    return <div className="h-full w-full" data-testid="metric-card-sparkline-empty" />
  }

  return <SparklinePlot className="h-full w-full" color={`var(${accent})`} data={points} dataKey="v" />
}
