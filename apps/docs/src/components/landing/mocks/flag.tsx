import { type ReactNode, useId } from 'react'

import type { CountryCode } from '../mock-data'

/* Simplified flags, each drawn in the 14 x 10 box at (1, 2) of a 20 x 16 viewBox. The svg keeps the box of the
   emoji glyph it replaces (1.25em by 1em, see .sv-flag) and the flag sits where the Apple emoji painted it.
   Stripes overlap by a fraction so no background shows through between them. */
const drawings: Record<CountryCode, ReactNode> = {
  de: (
    <>
      <rect fill="#000" height="3.5" width="14" x="1" y="2" />
      <rect fill="#dd0000" height="3.5" width="14" x="1" y="5.333" />
      <rect fill="#ffce00" height="3.333" width="14" x="1" y="8.667" />
    </>
  ),
  hk: (
    <>
      <rect fill="#de2910" height="10" width="14" x="1" y="2" />
      {/* The bauhinia: one curled petal, turned five times around the centre. */}
      <g fill="#fff" transform="translate(8 7)">
        {[0, 72, 144, 216, 288].map((angle) => (
          <path d="M0-.3C1.2-.8 1.5-2.5.4-3.1-.5-2.8-.9-1.4 0-.3Z" key={angle} transform={`rotate(${angle})`} />
        ))}
      </g>
    </>
  ),
  jp: (
    <>
      <rect fill="#fff" height="10" width="14" x="1" y="2" />
      <circle cx="8" cy="7" fill="#bc002d" r="3" />
    </>
  ),
  sg: (
    <>
      <rect fill="#fff" height="10" width="14" x="1" y="2" />
      <rect fill="#ef3340" height="5" width="14" x="1" y="2" />
      {/* Crescent and five stars, as dots at this size. */}
      <circle cx="4.1" cy="4.5" fill="#fff" r="1.9" />
      <circle cx="4.9" cy="4.5" fill="#ef3340" r="1.7" />
      <g fill="#fff">
        <circle cx="5.6" cy="3.5" r="0.35" />
        <circle cx="6.55" cy="4.2" r="0.35" />
        <circle cx="6.2" cy="5.3" r="0.35" />
        <circle cx="5" cy="5.3" r="0.35" />
        <circle cx="4.65" cy="4.2" r="0.35" />
      </g>
    </>
  ),
  us: (
    <>
      {/* Seven stripes instead of thirteen, which would blur at this size. */}
      <rect fill="#b22234" height="10" width="14" x="1" y="2" />
      <g fill="#fff">
        <rect height="1.429" width="14" x="1" y="3.429" />
        <rect height="1.429" width="14" x="1" y="6.286" />
        <rect height="1.429" width="14" x="1" y="9.143" />
      </g>
      <rect fill="#3c3b6e" height="5.714" width="6.2" x="1" y="2" />
      <g fill="#fff">
        <circle cx="2.2" cy="3.1" r="0.4" />
        <circle cx="4.1" cy="3.1" r="0.4" />
        <circle cx="6" cy="3.1" r="0.4" />
        <circle cx="3.15" cy="4.85" r="0.4" />
        <circle cx="5.05" cy="4.85" r="0.4" />
        <circle cx="2.2" cy="6.6" r="0.4" />
        <circle cx="4.1" cy="6.6" r="0.4" />
        <circle cx="6" cy="6.6" r="0.4" />
      </g>
    </>
  )
}

/**
 * A country flag drawn in SVG, with the emoji look: rounded corners, a soft top-to-bottom shade and a faint
 * edge that keeps white flags visible on white cards. Regional-indicator emoji made WebKit stall for seconds
 * in system font fallback, so the mocks draw their flags instead.
 */
export function Flag({ className, code }: { className: string; code: CountryCode }) {
  const id = useId()
  return (
    <svg aria-hidden="true" className={className} viewBox="0 0 20 16">
      <defs>
        <clipPath id={`${id}clip`}>
          <rect height="10" rx="1.5" width="14" x="1" y="2" />
        </clipPath>
        <linearGradient id={`${id}shade`} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.22" />
          <stop offset="0.5" stopColor="#fff" stopOpacity="0" />
          <stop offset="1" stopColor="#000" stopOpacity="0.14" />
        </linearGradient>
      </defs>
      <g clipPath={`url(#${id}clip)`}>
        {drawings[code]}
        <rect fill={`url(#${id}shade)`} height="10" width="14" x="1" y="2" />
        <rect fill="none" height="10" rx="1.5" stroke="#000" strokeOpacity="0.16" width="14" x="1" y="2" />
      </g>
    </svg>
  )
}
