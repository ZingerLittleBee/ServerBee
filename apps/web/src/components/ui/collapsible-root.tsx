'use client'

import { Collapsible as CollapsiblePrimitive } from '@base-ui/react/collapsible'

export function Collapsible({ ...props }: CollapsiblePrimitive.Root.Props) {
  return <CollapsiblePrimitive.Root data-slot="collapsible" {...props} />
}
