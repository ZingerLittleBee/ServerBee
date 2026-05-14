import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  getLatencySquareColor,
  getLossSquareColor,
  isLatencyFailure,
  LATENCY_UNKNOWN_BAR_COLOR
} from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import type { ServerCardMetricPoint } from './server-card-network-data'

const SQUARE_SIZE = 6
const SQUARE_GAP = 2
const STEP = SQUARE_SIZE + SQUARE_GAP

interface NetworkSquareGridProps {
  kind: 'latency' | 'loss'
  points: readonly ServerCardMetricPoint[]
}

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }
  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
}

function getSquareColor(point: ServerCardMetricPoint, kind: 'latency' | 'loss'): string {
  if (point.synthetic) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (kind === 'latency') {
    return getLatencySquareColor({ latencyMs: point.value, lossRatio: averageLossRatio(point) })
  }
  return getLossSquareColor(point.value)
}

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '-'
  }
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '-'
  }
  return `${(lossRatio * 100).toFixed(1)}%`
}

function formatTooltipLabel(point: ServerCardMetricPoint, t: (key: string) => string): string {
  if (point.synthetic) {
    return t('current_targets')
  }
  return new Date(point.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < 0.01) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (lossRatio < 0.05) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function PointTooltip({ point, t }: { point: ServerCardMetricPoint; t: (key: string) => string }) {
  if (point.targets.length === 0) {
    return null
  }
  return (
    <>
      <div className="font-medium">{formatTooltipLabel(point, t)}</div>
      <div className="grid gap-1.5">
        {point.targets.map((target) => {
          const failed = isLatencyFailure(target.lossRatio)
          return (
            <div className="flex items-center justify-between gap-3" key={target.targetId}>
              <span className="truncate text-muted-foreground">{target.targetName}</span>
              <div className="flex gap-2 font-medium font-mono tabular-nums">
                <span className={latencyColorClass(target.latency, { failed })}>{formatLatency(target.latency)}</span>
                <span className={getLossTextClassName(target.lossRatio)}>{formatPacketLoss(target.lossRatio)}</span>
              </div>
            </div>
          )
        })}
      </div>
    </>
  )
}

export function NetworkSquareGrid({ points, kind }: NetworkSquareGridProps) {
  const { t } = useTranslation(['servers'])
  const containerRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)

  useEffect(() => {
    const el = containerRef.current
    if (!el) {
      return
    }
    const observer = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0
      setWidth(w)
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const maxSquares = Math.max(1, Math.floor((width + SQUARE_GAP) / STEP))
  const visible = points.slice(-maxSquares)

  return (
    <div className="flex h-3 w-full items-end overflow-hidden" ref={containerRef} style={{ gap: `${SQUARE_GAP}px` }}>
      {visible.map((point) => (
        <Tooltip key={point.timestamp}>
          <TooltipTrigger
            render={
              <div
                className="flex-none rounded-[1px]"
                data-testid="square"
                style={{
                  backgroundColor: getSquareColor(point, kind),
                  height: `${SQUARE_SIZE}px`,
                  width: `${SQUARE_SIZE}px`
                }}
              />
            }
          />
          <TooltipContent
            className="grid min-w-48 gap-1.5 rounded-lg border border-border/50 bg-background/95 px-3 py-2 text-xs shadow-xl backdrop-blur-sm"
            sideOffset={4}
          >
            <PointTooltip point={point} t={t} />
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  )
}
