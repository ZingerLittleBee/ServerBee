import { formatBytes, formatSpeed } from '@/lib/utils'

type MetricValueKind = 'bytes' | 'speed'
type MetricValueVariant = 'compact' | 'dense'

interface MetricValueProps {
  kind: MetricValueKind
  value: number
  variant?: MetricValueVariant
}

function splitValueUnit(formatted: string): { unit: string | null; value: string } {
  const lastSpace = formatted.lastIndexOf(' ')
  if (lastSpace < 0) {
    return { unit: null, value: formatted }
  }
  return { unit: formatted.slice(lastSpace + 1), value: formatted.slice(0, lastSpace) }
}

function denseValueClassName(value: string): string {
  return value === '0' ? 'text-xs' : 'font-semibold text-foreground text-xs'
}

export function MetricValue({ kind, value, variant = 'dense' }: MetricValueProps) {
  if (kind === 'speed' && value <= 0) {
    return variant === 'compact' ? '0' : <span className="text-xs">0</span>
  }

  const formatted = kind === 'bytes' ? formatBytes(value) : formatSpeed(value)
  const parts = splitValueUnit(formatted)

  if (parts.unit == null) {
    return variant === 'compact' ? parts.value : <span className={denseValueClassName(parts.value)}>{parts.value}</span>
  }

  if (variant === 'compact') {
    return (
      <>
        {parts.value}
        <span className="ml-0.5 font-normal text-[10px] text-muted-foreground">{parts.unit}</span>
      </>
    )
  }

  return (
    <>
      <span className={denseValueClassName(parts.value)}>{parts.value}</span>{' '}
      <span className="text-[9px]">{parts.unit}</span>
    </>
  )
}
