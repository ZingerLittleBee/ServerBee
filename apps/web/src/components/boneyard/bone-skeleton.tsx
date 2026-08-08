import { Skeleton, type SkeletonProps } from 'boneyard-js/react'
import { Skeleton as Placeholder } from '@/components/ui/skeleton'
import { useReducedMotion } from '@/hooks/use-reduced-motion'
import { cn } from '@/lib/utils'

/**
 * Visible last-resort placeholder shown when a named skeleton has no
 * registered bones (stale/missing registry, partial generation, name drift).
 * Generic inert blocks only — no fake server/monitor values, no focusable
 * controls — so it can never masquerade as real content, and `aria-hidden`
 * keeps it out of the accessibility tree (the container's `aria-busy`
 * already conveys loading).
 *
 * Reduced motion: the shadcn primitive carries an unconditional base
 * `animate-pulse`, and a `motion-safe:` variant would merely coexist with it
 * in the merged class list (the base class still applies). Passing
 * `animate-none` / `animate-pulse` as the trailing class makes
 * tailwind-merge drop the base class outright, so reduced-motion users get a
 * fully static fallback while everyone else keeps exactly one pulse.
 */
function DefaultFallback() {
  const reducedMotion = useReducedMotion()
  const motion = reducedMotion ? 'animate-none' : 'animate-pulse'
  return (
    <div aria-hidden="true" className="space-y-4" data-boneyard-fallback="true">
      <Placeholder className={cn('h-8 w-1/3', motion)} />
      <Placeholder className={cn('h-4 w-2/3', motion)} />
      <div className="grid gap-4 sm:grid-cols-2">
        <Placeholder className={cn('h-40', motion)} />
        <Placeholder className={cn('h-40', motion)} />
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
 *   generic placeholder blocks keep the page visibly loading. The fallback
 *   pulses when motion is allowed and is fully static under reduced motion.
 */
export function BoneSkeleton({
  animate,
  fallback = <DefaultFallback />,
  select = 'viewport',
  ...props
}: SkeletonProps) {
  const reducedMotion = useReducedMotion()
  return <Skeleton animate={reducedMotion ? 'solid' : animate} fallback={fallback} select={select} {...props} />
}

export type BoneSkeletonProps = SkeletonProps
