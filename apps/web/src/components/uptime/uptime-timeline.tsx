import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar } from '@/components/charts/bar'
import { BarChart } from '@/components/charts/bar-chart'
import { sampleChartRows } from '@/components/charts/chart-accessibility'
import { ChartTooltip } from '@/components/charts/tooltip/chart-tooltip'
import { TooltipContent } from '@/components/charts/tooltip/tooltip-content'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeUptimeColor, formatUptimeTooltip, type UptimeColor } from '@/lib/widget-helpers'
import { SEGMENT_COLOR_VALUE_MAP, STATUS_HISTORY_COLOR_VALUE_MAP } from './uptime-timeline-colors'

export type UptimeTimelineAppearance = 'default' | 'status-history'

export interface UptimeTimelineProps {
  appearance?: UptimeTimelineAppearance
  days: UptimeDailyEntry[]
  height?: number
  rangeDays: number
  redThreshold?: number
  showLabels?: boolean
  showLegend?: boolean
  yellowThreshold?: number
}

/** Every day fills the full plot height; only its color carries the status. */
const FULL_HEIGHT_VALUE = 1
const VALUE_DOMAIN: [number, number] = [0, FULL_HEIGHT_VALUE]

/** Gaps between day columns as a fraction of the band. */
const DAY_GAP_RATIOS: Record<UptimeTimelineAppearance, number> = {
  default: 0.12,
  'status-history': 0.3
}

const STATUS_ORDER: UptimeColor[] = ['green', 'yellow', 'red', 'gray']

const LEGEND_LABEL_KEYS: Record<UptimeColor, string> = {
  green: 'uptime_operational',
  yellow: 'uptime_degraded',
  red: 'uptime_down',
  gray: 'uptime_no_data'
}

interface TimelineRow extends Record<string, unknown> {
  entry: UptimeDailyEntry
  label: string
  status: UptimeColor
}

function buildRows(
  days: UptimeDailyEntry[],
  rangeDays: number,
  yellowThreshold: number,
  redThreshold: number
): TimelineRow[] {
  const slice = days.slice(-rangeDays)
  const padCount = Math.max(0, rangeDays - slice.length)
  const padded: UptimeDailyEntry[] = Array.from({ length: padCount }, () => ({
    date: '',
    online_minutes: 0,
    total_minutes: 0,
    downtime_incidents: 0
  }))

  return [...padded, ...slice].map((entry, index) => {
    const status = computeUptimeColor(entry.online_minutes, entry.total_minutes, yellowThreshold, redThreshold)
    return {
      entry,
      label: entry.date || `pad-${index.toString()}`,
      status,
      [status]: FULL_HEIGHT_VALUE
    }
  })
}

export function UptimeTimeline({
  appearance = 'default',
  days,
  rangeDays,
  yellowThreshold = 100,
  redThreshold = 95,
  showLabels = false,
  showLegend = false,
  height = 28
}: UptimeTimelineProps) {
  const { t } = useTranslation('status')

  const rows = useMemo(
    () => buildRows(days, rangeDays, yellowThreshold, redThreshold),
    [days, rangeDays, yellowThreshold, redThreshold]
  )
  const accessibleRows = useMemo(() => sampleChartRows(rows), [rows])
  const timelineTitle = `${t('uptime_days_ago', { count: rangeDays })} - ${t('uptime_today')}`
  const colorValueMap = appearance === 'status-history' ? STATUS_HISTORY_COLOR_VALUE_MAP : SEGMENT_COLOR_VALUE_MAP

  return (
    <div className="w-full">
      {showLabels && (
        <div className="mb-1 flex justify-between text-muted-foreground text-xs">
          <span>{t('uptime_days_ago', { count: rangeDays })}</span>
          <span>{t('uptime_today')}</span>
        </div>
      )}

      <figure aria-label={timelineTitle} className="w-full" data-uptime-timeline="" style={{ margin: 0 }}>
        <div aria-hidden="true" data-testid="bklit-uptime-timeline" style={{ height }}>
          <BarChart
            aspectRatio=""
            barGap={DAY_GAP_RATIOS[appearance]}
            className="h-full w-full"
            data={rows}
            margin={{ top: 0, right: 0, bottom: 0, left: 0 }}
            stacked
            valueDomain={VALUE_DOMAIN}
            xDataKey="label"
          >
            <ChartTooltip
              content={({ point }) => {
                const entry = point.entry as UptimeDailyEntry | undefined
                if (!entry?.date) {
                  return <TooltipContent rows={[]} title={t('uptime_no_data')} />
                }
                const tooltip = formatUptimeTooltip(entry)
                const status = point.status as UptimeColor
                return (
                  <TooltipContent
                    rows={[
                      {
                        color: colorValueMap[status],
                        label: t(LEGEND_LABEL_KEYS[status]),
                        value: tooltip.percentage
                      }
                    ]}
                    title={tooltip.date}
                  >
                    <p className="text-chart-tooltip-muted text-xs">
                      {tooltip.duration} &middot; {tooltip.incidents}
                    </p>
                  </TooltipContent>
                )
              }}
              showDatePill={false}
            />
            {STATUS_ORDER.map((status) => (
              <Bar animate={false} dataKey={status} fill={colorValueMap[status]} key={status} lineCap={0} />
            ))}
          </BarChart>
        </div>

        <table className="sr-only">
          <caption>{timelineTitle}</caption>
          <thead>
            <tr>
              <th scope="col">{t('uptime_date')}</th>
              <th scope="col">{t('uptime_status')}</th>
            </tr>
          </thead>
          <tbody>
            {accessibleRows.map((row) => {
              const { entry, label, status } = row as TimelineRow
              return (
                <tr key={label}>
                  <td>{entry.date || t('uptime_no_data')}</td>
                  <td>{t(LEGEND_LABEL_KEYS[status])}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </figure>

      {showLegend && (
        <div className="mt-2 flex gap-4 text-muted-foreground text-xs">
          {STATUS_ORDER.map((status) => (
            <span className="flex items-center gap-1" key={status}>
              <span
                className="inline-block size-2.5 rounded-[2px]"
                style={{ backgroundColor: colorValueMap[status] }}
              />
              {t(LEGEND_LABEL_KEYS[status])}
            </span>
          ))}
        </div>
      )}
    </div>
  )
}
