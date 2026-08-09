import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export function PageBody({ className, ...props }: ComponentProps<'div'>) {
  return (
    <div className={cn('flex min-h-full min-w-0 flex-col p-3 sm:p-4', className)} data-slot="page-body" {...props} />
  )
}
