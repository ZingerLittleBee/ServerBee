interface UptimeBarProps {
  ariaLabel?: string
  data: (number | null)[]
  getColor: (value: number | null) => string
  height?: number
  maxValue?: number
}

const MIN_HEIGHT_PCT = 10

export function UptimeBar({ data, height = 16, getColor, maxValue, ariaLabel }: UptimeBarProps) {
  const effectiveMax = maxValue ?? data.reduce<number>((max, v) => (v != null && v > max ? v : max), 0)

  function barHeight(value: number | null): string {
    if (value == null) {
      return `${MIN_HEIGHT_PCT}%`
    }
    if (effectiveMax <= 0) {
      return `${MIN_HEIGHT_PCT}%`
    }
    const pct = (value / effectiveMax) * 100
    return `${Math.min(100, Math.max(MIN_HEIGHT_PCT, pct))}%`
  }

  return (
    <div aria-label={ariaLabel} role="img" style={{ display: 'flex', gap: '2px', height, alignItems: 'flex-end' }}>
      {data.map((value, i) => (
        <div
          data-testid="uptime-bar-item"
          // biome-ignore lint/suspicious/noArrayIndexKey: index is stable for temporal data that doesn't reorder
          key={i}
          style={{
            flex: 1,
            borderRadius: '2px',
            backgroundColor: getColor(value),
            height: barHeight(value)
          }}
        />
      ))}
    </div>
  )
}
