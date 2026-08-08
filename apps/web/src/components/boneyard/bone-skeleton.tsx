import { Skeleton, type SkeletonProps } from 'boneyard-js/react'
import { useReducedMotion } from '@/hooks/use-reduced-motion'

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
 * - `fallback` defaults to `null`: when bones are not registered (e.g. the
 *   generated registry failed to load) the fixture children must NOT become
 *   visible — they carry deterministic fake content that would masquerade as
 *   real data. An empty `aria-busy` container degrades more honestly.
 */
export function BoneSkeleton({ animate, fallback = null, select = 'viewport', ...props }: SkeletonProps) {
  const reducedMotion = useReducedMotion()
  return <Skeleton animate={reducedMotion ? 'solid' : animate} fallback={fallback} select={select} {...props} />
}

export type { SkeletonProps as BoneSkeletonProps }
