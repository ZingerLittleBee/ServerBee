'use client'

import type { VariantProps } from 'class-variance-authority'
import type * as React from 'react'
import { cn } from '@/lib/utils'
import { inputGroupAddonVariants, isElement } from './input-group-utils'

export function InputGroupAddon({
  className,
  align = 'inline-start',
  ...props
}: React.ComponentProps<'div'> & VariantProps<typeof inputGroupAddonVariants>) {
  return (
    <div
      className={cn(inputGroupAddonVariants({ align }), className)}
      data-align={align}
      data-slot="input-group-addon"
      onPointerDown={(e) => {
        if (isElement(e.target) && e.target.closest('button')) {
          return
        }
        e.currentTarget.parentElement?.querySelector('input')?.focus()
      }}
      {...props}
    />
  )
}
