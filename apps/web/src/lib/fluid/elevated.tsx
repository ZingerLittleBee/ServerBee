'use client'

import { type ComponentPropsWithoutRef, forwardRef, type ReactNode } from 'react'
import { surfaceClasses } from '@/lib/fluid/surface-classes'
import { SurfaceProvider, useSurface } from '@/lib/fluid/surface-context'
import { cn } from '@/lib/utils'

interface ElevatedProps extends ComponentPropsWithoutRef<'div'> {
  children?: ReactNode
  /**
   * Steps above the current substrate.
   *
   * The component's own surface level becomes `min(substrate + offset, 8)`
   * and is re-provided to descendants via SurfaceProvider, so further
   * nesting walks up the ladder automatically.
   *
   * Conventional offsets:
   *   2 — dropdown / popover / select menu
   *   4 — dialog / modal
   */
  offset: number
  /**
   * Override for the shadow level. Defaults to the computed surface level.
   *
   * Pass a fixed value when the component should keep a constant shadow
   * weight regardless of how deeply it's nested — e.g. a dropdown always
   * reads `shadow-surface-3` whether it opens on the page or inside a
   * dialog, even though its background tracks the substrate.
   */
  shadowLevel?: number
}

const Elevated = forwardRef<HTMLDivElement, ElevatedProps>(
  ({ offset, shadowLevel, className, children, ...props }, ref) => {
    const substrate = useSurface()
    const level = Math.min(substrate + offset, 8)
    return (
      <SurfaceProvider value={level}>
        <div className={cn(surfaceClasses(level, shadowLevel ?? level), className)} ref={ref} {...props}>
          {children}
        </div>
      </SurfaceProvider>
    )
  }
)
Elevated.displayName = 'Elevated'

export { Elevated }
