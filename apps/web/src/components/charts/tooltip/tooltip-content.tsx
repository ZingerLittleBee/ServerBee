'use client'

import type { ReactNode } from 'react'
import { intFmt } from '../chart-formatters'

export interface TooltipRow {
  color: string
  label: string
  value: string | number
}

export interface TooltipContentProps {
  /** Optional additional content (e.g., markers) */
  children?: ReactNode
  rows: TooltipRow[]
  title?: string
}

export function TooltipContent({ title, rows, children }: TooltipContentProps) {
  return (
    <div className="overflow-hidden">
      <div className="px-3 py-2.5">
        {title && <div className="mb-2 text-left font-medium text-chart-tooltip-foreground text-xs">{title}</div>}
        <div className="space-y-1.5">
          {/* Local fix: label+color is not unique (a chart may repeat a label,
              e.g. "No data" twice), and duplicate keys left stale rows behind
              when the hovered point changed. */}
          {rows.map((row, index) => (
            <div className="flex items-center justify-between gap-4" key={`${index.toString()}-${row.label}`}>
              <div className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ backgroundColor: row.color }} />
                <span className="text-chart-tooltip-muted text-sm">{row.label}</span>
              </div>
              <span className="font-medium text-chart-tooltip-foreground text-sm tabular-nums">
                {typeof row.value === 'number' ? intFmt(row.value) : row.value}
              </span>
            </div>
          ))}
        </div>

        {children && <div className="mt-2 transition-opacity duration-200 ease-out">{children}</div>}
      </div>
    </div>
  )
}

TooltipContent.displayName = 'TooltipContent'

export default TooltipContent
