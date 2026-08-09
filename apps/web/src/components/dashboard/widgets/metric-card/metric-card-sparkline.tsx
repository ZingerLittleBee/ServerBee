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

  // `accent` is a full CSS color (`#RRGGBB` or `var(--chart-1)`).
  return <SparklinePlot className="h-full w-full" color={accent} data={points} dataKey="v" />
}
