import type { ReactElement } from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { NetworkTargetBreakdown } from './network-target-breakdown'
import type { ServerCardTooltipTarget } from './server-card-network-data'

export function NetworkMetricValue({
  children,
  targets
}: {
  children: ReactElement
  targets: readonly ServerCardTooltipTarget[]
}) {
  if (targets.length === 0) {
    return children
  }
  return (
    <Tooltip>
      <TooltipTrigger render={children} />
      <TooltipContent className="grid min-w-48 gap-1.5" sideOffset={4}>
        <NetworkTargetBreakdown targets={targets} />
      </TooltipContent>
    </Tooltip>
  )
}
