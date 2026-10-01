import type { FeatureCopy, LinkCopy, PointCopy } from '../translations'
import { LandingLink } from './landing-link'
import { typeset } from './typeset'

/**
 * "Docs →" link. The label and the arrow are separate flex items, so the flex gap separates them as the design's
 * template holes did. The arrow is decorative and stays out of the accessible name.
 */
export function MoreLink({ link }: { link: LinkCopy }) {
  return (
    <LandingLink className="more-link" href={link.href}>
      <span>{link.label}</span>
      <span aria-hidden="true">→</span>
    </LandingLink>
  )
}

/** Heading, lede and docs link above a full-width panel (`.sec-head`). */
export function SectionHead({ copy }: { copy: FeatureCopy }) {
  return (
    <div className="sec-head">
      <div className="sec-head-copy">
        <h2 className="h2">{copy.h2}</h2>
        <p className="lede">{typeset(copy.lede)}</p>
      </div>
      <MoreLink link={copy.link} />
    </div>
  )
}

/** Titled points; `className` picks the layout: `points`, `pts-row`, `pts-row four` or `sec-pts`. */
export function Points({ className, points }: { className: string; points: PointCopy[] }) {
  return (
    <ul className={className}>
      {points.map((point) => (
        <li key={point.t}>
          <span className="pt-t">{point.t}</span>
          <span className="pt-d">{typeset(point.d)}</span>
        </li>
      ))}
    </ul>
  )
}
