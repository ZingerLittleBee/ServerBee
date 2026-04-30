import { useEffect, useState } from 'react'
import { Input } from '@/components/ui/input'
import { formatOklch, hexToOklch, oklchToHex, parseOklch } from '@/lib/oklch'

const HEX_COLOR_RE = /^#[0-9a-fA-F]{6}$/

interface OklchPickerProps {
  onChange: (next: string) => void
  showHex?: boolean
  value: string
}

export function OklchPicker({ onChange, showHex = true, value }: OklchPickerProps) {
  const parsed = parseOklch(value) ?? { c: 0.1, h: 0, l: 0.5 }
  const [hex, setHex] = useState(() => oklchToHex(value) ?? '')

  useEffect(() => {
    const nextHex = oklchToHex(value)
    if (nextHex) {
      setHex(nextHex)
    }
  }, [value])

  const update = (next: Partial<typeof parsed>) => {
    onChange(formatOklch({ ...parsed, ...next }))
  }

  const onHexChange = (nextHex: string) => {
    setHex(nextHex)
    if (HEX_COLOR_RE.test(nextHex)) {
      const oklch = hexToOklch(nextHex)
      if (oklch) {
        onChange(oklch)
      }
    }
  }

  return (
    <div className="flex items-center gap-2">
      <div className="size-6 rounded border" style={{ background: value }} />
      <div className="grid flex-1 grid-cols-3 gap-1">
        <ChannelSlider label="L" max={1} min={0} onChange={(l) => update({ l })} step={0.01} value={parsed.l} />
        <ChannelSlider label="C" max={0.5} min={0} onChange={(c) => update({ c })} step={0.005} value={parsed.c} />
        <ChannelSlider label="H" max={360} min={0} onChange={(h) => update({ h })} step={1} value={parsed.h} />
      </div>
      {showHex && (
        <Input
          className="w-24"
          onChange={(event) => onHexChange(event.target.value)}
          placeholder="#rrggbb"
          value={hex}
        />
      )}
    </div>
  )
}

function ChannelSlider({
  label,
  max,
  min,
  onChange,
  step,
  value
}: {
  label: string
  max: number
  min: number
  onChange: (value: number) => void
  step: number
  value: number
}) {
  return (
    <label className="flex items-center gap-1 text-xs">
      <span className="w-3 text-muted-foreground">{label}</span>
      <input
        aria-label={label}
        className="flex-1"
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.target.value))}
        step={step}
        type="range"
        value={value}
      />
      <span className="w-10 text-right tabular-nums">{value.toFixed(label === 'H' ? 0 : 2)}</span>
    </label>
  )
}
