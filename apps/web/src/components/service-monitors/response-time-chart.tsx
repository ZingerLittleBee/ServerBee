import { useId, useMemo } from 'react'
import { Area } from '@/components/charts/area'
import { AreaChart } from '@/components/charts/area-chart'
import { Grid } from '@/components/charts/grid'
import { ChartTooltip } from '@/components/charts/tooltip/chart-tooltip'
import { TooltipContent } from '@/components/charts/tooltip/tooltip-content'
import { XAxis } from '@/components/charts/x-axis'
import { YAxis } from '@/components/charts/y-axis'
import { formatDateTime } from '@/lib/format'

interface ResponseTimeRecord {
  latency: number | null
  success: boolean
  time: string
}

interface ResponseTimeChartProps {
  records: ResponseTimeRecord[]
  t: (key: string) => string
}

const LATENCY_COLOR = 'var(--chart-4)'

function formatAxisTime(date: Date): string {
  return date.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false
  })
}

function formatLatency(value: unknown, failedLabel: string): string {
  return typeof value === 'number' && Number.isFinite(value) ? `${value.toFixed(1)} ms` : failedLabel
}

export function ResponseTimeChart({ records, t }: ResponseTimeChartProps) {
  const titleId = useId()
  const chartData = useMemo(
    () =>
      records.toReversed().map((record) => ({
        latency: record.success ? record.latency : null,
        success: record.success,
        timestamp: record.time
      })),
    [records]
  )

  if (chartData.length === 0) {
    return (
      <div className="rounded-lg border bg-card p-6 text-center text-muted-foreground text-sm">
        {t('chart.noRecords')}
      </div>
    )
  }

  const failedLabel = t('history.status.fail')

  return (
    <figure aria-labelledby={titleId} className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm" id={titleId}>
        {t('chart.responseTime')}
      </h3>
      <div aria-hidden="true" data-testid="bklit-response-time-chart">
        <AreaChart
          animationDuration={600}
          aspectRatio=""
          className="h-[260px] w-full"
          data={chartData}
          margin={{ left: 50, right: 16, top: 8, bottom: 36 }}
          xDataKey="timestamp"
          yDomainTweenDuration={300}
        >
          <Grid vertical={false} />
          <XAxis formatValue={formatAxisTime} numTicks={5} />
          <YAxis formatValue={(value) => `${value.toFixed(0)} ms`} />
          <ChartTooltip
            content={({ point }) => (
              <TooltipContent
                rows={[
                  {
                    color: LATENCY_COLOR,
                    label: t('chart.latency'),
                    value: formatLatency(point.latency, failedLabel)
                  }
                ]}
                title={formatDateTime(String(point.timestamp), { hour12: false })}
              />
            )}
            showDatePill={false}
          />
          <Area dataKey="latency" fill={LATENCY_COLOR} fillOpacity={0.1} stroke={LATENCY_COLOR} strokeWidth={2} />
        </AreaChart>
      </div>

      <table className="sr-only">
        <caption>{t('chart.responseTime')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('history.table.time')}</th>
            <th scope="col">{t('history.table.status')}</th>
            <th scope="col">{t('history.table.latency')}</th>
          </tr>
        </thead>
        <tbody>
          {chartData.map((point) => (
            <tr key={point.timestamp}>
              <td>{formatDateTime(point.timestamp, { hour12: false })}</td>
              <td>{t(point.success ? 'history.status.ok' : 'history.status.fail')}</td>
              <td>{formatLatency(point.latency, failedLabel)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  )
}
