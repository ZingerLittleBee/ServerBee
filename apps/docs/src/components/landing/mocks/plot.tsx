import type { ChartLine } from '../model/shared'

interface GridLineProps {
  /** The solid bottom line. */
  base?: boolean
  /** Axis label (`<span>`); omitted on the network chart's base line. */
  label?: string
  /** Vertical position inside `.plot` / `.ios-plot`, e.g. `33.333%`. */
  top: string
}

/** One horizontal grid line of a chart plot. */
export function GridLine({ top, label, base = false }: GridLineProps) {
  return (
    <div className={base ? 'gl base' : 'gl'} style={{ top }}>
      {label === undefined ? null : <span>{label}</span>}
    </div>
  )
}

/** Chart series as stacked `.plot-svg` polylines in a 0..100 viewBox, colored by `style.color`. */
export function PlotLines({ lines }: { lines: readonly ChartLine[] }) {
  return lines.map((line) => (
    <svg
      aria-hidden="true"
      className="plot-svg"
      key={line.id}
      preserveAspectRatio="none"
      style={line.style}
      viewBox="0 0 100 100"
    >
      <polyline className="ln" points={line.pts} />
    </svg>
  ))
}
