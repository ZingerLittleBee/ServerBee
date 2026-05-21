import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { cn } from '@/lib/utils'
import { computeUptimeColor, formatUptimeTooltip, type UptimeColor } from '@/lib/widget-helpers'

export interface UptimeTimelineProps {
  days: UptimeDailyEntry[]
  height?: number
  rangeDays: number
  redThreshold?: number
  showLabels?: boolean
  showLegend?: boolean
  yellowThreshold?: number
}

const COLOR_MAP: Record<UptimeColor, string> = {
  green: 'bg-emerald-500',
  yellow: 'bg-amber-500',
  red: 'bg-red-500',
  gray: 'bg-muted'
}

export function UptimeTimeline({
  days,
  rangeDays,
  yellowThreshold = 100,
  redThreshold = 95,
  showLabels = false,
  showLegend = false,
  height = 28
}: UptimeTimelineProps) {
  const { t } = useTranslation('status')
  const segments = useMemo(() => {
    const slice = days.slice(-rangeDays)
    const padCount = rangeDays - slice.length
    const padded: UptimeDailyEntry[] = Array.from({ length: padCount }, () => ({
      date: '',
      online_minutes: 0,
      total_minutes: 0,
      downtime_incidents: 0
    }))
    return [...padded, ...slice]
  }, [days, rangeDays])

  return (
    <div className="w-full">
      {showLabels && (
        <div className="mb-1 flex justify-between text-muted-foreground text-xs">
          <span>{t('uptime_days_ago', { count: rangeDays })}</span>
          <span>{t('uptime_today')}</span>
        </div>
      )}
      <div className="relative flex w-full" style={{ height, gap: '1.5px' }}>
        {segments.map((entry, i) => {
          const color = computeUptimeColor(entry.online_minutes, entry.total_minutes, yellowThreshold, redThreshold)
          const tooltip = formatUptimeTooltip(entry)
          return (
            <Tooltip key={entry.date || `pad-${i.toString()}`}>
              <TooltipTrigger
                className={cn('flex-1 rounded-[2px] focus:outline-none', COLOR_MAP[color])}
                data-segment={color}
                type="button"
              />
              <TooltipContent>
                <div className="text-xs">
                  <p className="font-medium">{tooltip.date || t('uptime_no_data')}</p>
                  <p className="text-muted-foreground">
                    {tooltip.percentage} &middot; {tooltip.duration}
                  </p>
                  <p className="text-muted-foreground">{tooltip.incidents}</p>
                </div>
              </TooltipContent>
            </Tooltip>
          )
        })}
      </div>
      {showLegend && (
        <div className="mt-2 flex gap-4 text-muted-foreground text-xs">
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-[2px] bg-emerald-500" />
            {t('uptime_operational')}
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-[2px] bg-amber-500" />
            {t('uptime_degraded')}
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-[2px] bg-red-500" />
            {t('uptime_down')}
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-[2px] bg-muted" />
            {t('uptime_no_data')}
          </span>
        </div>
      )}
    </div>
  )
}
