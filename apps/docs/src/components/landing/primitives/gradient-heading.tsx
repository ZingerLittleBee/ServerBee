import type { PropsWithChildren } from 'react'

import { cn } from '@/lib/cn'

export function GradientHeading({
  as: Tag = 'h2',
  className,
  children
}: PropsWithChildren<{ as?: 'h1' | 'h2' | 'h3'; className?: string }>) {
  return (
    <Tag
      className={cn(
        'gradient-text text-balance font-semibold tracking-tight',
        Tag === 'h1' ? 'text-5xl leading-tight sm:text-6xl lg:text-7xl' : 'text-3xl sm:text-4xl',
        className
      )}
    >
      {children}
    </Tag>
  )
}
