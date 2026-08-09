import type { ReactElement } from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { NetworkTargetBreakdown } from './network-target-breakdown'
import type { ServerCardTooltipTarget } from './server-card-network-data'

export function NetworkMetricValue({
  children,
  targets,
  tooltips = true
}: {
  children: ReactElement
  targets: readonly ServerCardTooltipTarget[]
  /** When false (e.g. offline cards), render the value without a hover/focus breakdown. */
  tooltips?: boolean
}) {
  if (!(tooltips && targets.length > 0)) {
    return children
  }
  // Base UI's default trigger is a real button, so the per-target breakdown is reachable by
  // keyboard and not just on hover. The button is stripped back to the caller's own styling;
  // `inline-flex items-baseline` is what keeps it from synthesizing a button baseline, which
  // would otherwise push the value down inside the card's `items-baseline` rows.
  return (
    <Tooltip>
      <TooltipTrigger className="inline-flex cursor-default appearance-none items-baseline rounded-sm border-0 bg-transparent p-0 outline-none focus-visible:ring-2 focus-visible:ring-ring/50">
        {children}
      </TooltipTrigger>
      <TooltipContent className="grid min-w-48 gap-1.5" sideOffset={4}>
        <NetworkTargetBreakdown targets={targets} />
      </TooltipContent>
    </Tooltip>
  )
}
