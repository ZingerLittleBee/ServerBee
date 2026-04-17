import type { CombinedSeverity } from '@/lib/network-latency-constants'

export interface SeverityBarDatum {
  combinedSeverity: CombinedSeverity
  fillColor: string
  lossRatio: number | null
  value: number | null
}

interface SeverityBarProps {
  background?: { x: number; y: number; width: number; height: number }
  failPatternId: string
  height?: number
  payload?: SeverityBarDatum
  width?: number
  x?: number
  y?: number
}

export function SeverityBar({
  x = 0,
  y = 0,
  width = 0,
  height = 0,
  background,
  payload,
  failPatternId
}: SeverityBarProps) {
  if (!payload || width <= 0) {
    return null
  }

  const isFailed = payload.combinedSeverity === 'failed'
  const radius = 2

  if (isFailed) {
    const bgY = background?.y ?? y
    const bgHeight = background?.height ?? height
    return (
      <rect fill={`url(#${failPatternId})`} height={bgHeight} rx={radius} ry={radius} width={width} x={x} y={bgY} />
    )
  }

  const safeHeight = Math.max(height, 2)
  const safeY = y + (height - safeHeight)

  return <rect fill={payload.fillColor} height={safeHeight} rx={radius} ry={radius} width={width} x={x} y={safeY} />
}
