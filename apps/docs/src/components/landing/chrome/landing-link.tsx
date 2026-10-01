import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import type { LandingHref } from '../translations'

interface LandingLinkProps {
  children: ReactNode
  className?: string
  href: LandingHref
}

/** Documentation targets become client-side router links; anchors and external URLs stay plain links. */
export function LandingLink({ href, className, children }: LandingLinkProps) {
  if (typeof href === 'string') {
    return (
      <a className={className} href={href}>
        {children}
      </a>
    )
  }
  return (
    <Link className={className} hash={href.hash} params={{ lang: href.lang, _splat: href.page }} to="/$lang/docs/$">
      {children}
    </Link>
  )
}
