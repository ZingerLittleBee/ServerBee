import { Skeleton, type SkeletonProps } from 'boneyard-js/react'
import { Skeleton as Placeholder } from '@/components/ui/skeleton'
import { useReducedMotion } from '@/hooks/use-reduced-motion'

/**
 * Visible last-resort placeholder shown when a named skeleton has no
 * registered bones (stale/missing registry, partial generation, name drift).
 * Generic inert blocks only — no fake server/monitor values, no focusable
 * controls — so it can never masquerade as real content, and `aria-hidden`
 * keeps it out of the accessibility tree (the container's `aria-busy`
 * already conveys loading). `motion-safe:` keeps the pulse off for
 * reduced-motion users (tailwind-merge lets it override the base
 * `animate-pulse`).
 */
function DefaultFallback() {
  return (
    <div aria-hidden="true" className="space-y-4" data-boneyard-fallback="true">
      <Placeholder className="h-8 w-1/3 motion-safe:animate-pulse" />
      <Placeholder className="h-4 w-2/3 motion-safe:animate-pulse" />
      <div className="grid gap-4 sm:grid-cols-2">
        <Placeholder className="h-40 motion-safe:animate-pulse" />
        <Placeholder className="h-40 motion-safe:animate-pulse" />
      </div>
    </div>
  )
}

/**
 * ServerBee's seam over boneyard's `<Skeleton>`.
 *
 * - `select="viewport"`: bones are captured keyed by viewport width, and our
 *   app shells (status max-w-6xl container, authed sidebar inset) make the
 *   skeleton container narrower than the viewport. Viewport selection picks
 *   the capture whose responsive layout matches what CSS actually rendered
 *   (boneyard issue #92), instead of letting container width pick a
 *   narrower breakpoint's layout.
 * - Reduced motion: boneyard has no built-in `prefers-reduced-motion`
 *   handling, so this wrapper forces `animate="solid"` (no pulse/shimmer)
 *   when the user prefers reduced motion.
 * - `fallback` defaults to `DefaultFallback`: when bones are not registered
 *   the fixture children must NOT become visible (they carry deterministic
 *   fake content), but a permanently blank page is not acceptable either —
 *   generic placeholder blocks keep the page visibly loading.
 */
export function BoneSkeleton({ animate, fallback = <DefaultFallback />, select = 'viewport', ...props }: SkeletonProps) {
  const reducedMotion = useReducedMotion()
  return <Skeleton animate={reducedMotion ? 'solid' : animate} fallback={fallback} select={select} {...props} />
}

export type { SkeletonProps as BoneSkeletonProps }
