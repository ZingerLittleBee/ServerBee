import { useTranslation } from 'react-i18next'
import { MetricAreaPlot } from '@/components/charts/metric-area-plot'

interface MetricsChartProps {
  color?: string
  data: Record<string, unknown>[]
  dataKey: string
  domain?: [number, number]
  formatTick?: (value: number) => string
  formatTime?: (time: string) => string
  formatTooltipLabel?: (time: string) => string
  formatValue?: (value: number) => string
  title: string
  unit?: string
}

function defaultFormatTime(time: string): string {
  const d = new Date(time)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function defaultFormatValue(value: number): string {
  return value.toFixed(1)
}

export function MetricsChart({
  title,
  data,
  dataKey,
  color = 'var(--color-chart-1)',
  domain,
  unit = '',
  formatValue = defaultFormatValue,
  formatTick,
  formatTime = defaultFormatTime,
  formatTooltipLabel
}: MetricsChartProps) {
  const { t } = useTranslation('servers')
  const resolvedFormatTime = formatTime ?? defaultFormatTime
  const resolvedTooltipLabel = formatTooltipLabel ?? resolvedFormatTime

  return (
    <div className="min-w-0 rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title}</h3>
      <MetricAreaPlot
        ariaLabel={title}
        className="h-[260px] w-full"
        color={color}
        data={data}
        dataKey={dataKey}
        formatTime={resolvedFormatTime}
        formatTooltipLabel={resolvedTooltipLabel}
        formatValue={(value) => `${formatValue(value)}${unit}`}
        formatYAxisValue={(value) => {
          if (value === 0) {
            return ''
          }
          return formatTick ? formatTick(value) : String(value)
        }}
        timeLabel={t('chart_time')}
        valueLabel={title}
        yDomain={domain}
        yMarginLeft={formatTick ? 68 : 52}
      />
    </div>
  )
}
