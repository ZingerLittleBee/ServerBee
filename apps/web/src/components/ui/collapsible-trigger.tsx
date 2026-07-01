'use client'

import { Collapsible as CollapsiblePrimitive } from '@base-ui/react/collapsible'

export function CollapsibleTrigger({ ...props }: CollapsiblePrimitive.Trigger.Props) {
  return <CollapsiblePrimitive.Trigger data-slot="collapsible-trigger" {...props} />
}
