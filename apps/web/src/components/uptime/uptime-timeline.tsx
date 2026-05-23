import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
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

const POPUP_CLASS =
  'data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:fade-in-0 data-open:zoom-in-95 data-closed:fade-out-0 data-closed:zoom-out-95 z-50 inline-flex w-fit max-w-xs origin-(--transform-origin) flex-col rounded-md border bg-popover px-3 py-1.5 text-popover-foreground text-xs shadow-md data-[state=delayed-open]:animate-in data-closed:animate-out data-open:animate-in'

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

  // One handle per timeline instance — lets the 90 detached triggers share a
  // single tooltip popup instead of each spawning its own Root/Portal/Popup.
  const [handle] = useState(() => TooltipPrimitive.createHandle<UptimeDailyEntry>())

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

      <TooltipPrimitive.Root handle={handle}>
        {({ payload: entry }) => {
          const tooltip = entry ? formatUptimeTooltip(entry) : null
          return (
            <TooltipPrimitive.Portal>
              <TooltipPrimitive.Positioner align="center" className="isolate z-50" side="top" sideOffset={4}>
                <TooltipPrimitive.Popup className={POPUP_CLASS}>
                  <p className="font-medium">{tooltip?.date || t('uptime_no_data')}</p>
                  {tooltip && (
                    <>
                      <p className="text-muted-foreground">
                        {tooltip.percentage} &middot; {tooltip.duration}
                      </p>
                      <p className="text-muted-foreground">{tooltip.incidents}</p>
                    </>
                  )}
                </TooltipPrimitive.Popup>
              </TooltipPrimitive.Positioner>
            </TooltipPrimitive.Portal>
          )
        }}
      </TooltipPrimitive.Root>

      <div className="flex w-full" style={{ height, gap: '1.5px' }}>
        {segments.map((entry, i) => {
          const color = computeUptimeColor(entry.online_minutes, entry.total_minutes, yellowThreshold, redThreshold)
          return (
            <TooltipPrimitive.Trigger
              data-segment={color}
              handle={handle}
              key={entry.date || `pad-${i.toString()}`}
              payload={entry}
              render={<div className={cn('flex-1 rounded-[2px] focus:outline-none', COLOR_MAP[color])} />}
            />
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
