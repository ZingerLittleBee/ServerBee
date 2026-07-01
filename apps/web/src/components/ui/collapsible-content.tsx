'use client'

import { Collapsible as CollapsiblePrimitive } from '@base-ui/react/collapsible'

export function CollapsibleContent({ ...props }: CollapsiblePrimitive.Panel.Props) {
  return <CollapsiblePrimitive.Panel data-slot="collapsible-content" {...props} />
}
