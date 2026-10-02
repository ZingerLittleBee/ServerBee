/* Search and social copy for the landing page. The route head in routes/$lang/index.tsx turns it into tags. */

import { type LandingLang, landingCopy } from './translations'

export interface LandingSeoCopy {
  /** The meta description, also used for Open Graph and Twitter cards. */
  description: string
  title: string
}

export const landingSeo: Record<LandingLang, LandingSeoCopy> = {
  en: {
    description:
      'Self-hosted VPS monitoring with live metrics, network quality, IP reputation, alerts and costs in one panel, plus drag-and-drop dashboards and a native iOS app.',
    title: 'ServerBee: self-hosted VPS monitoring, down to every route'
  },
  zh: {
    // The hero subtitle doubles as the description, so the two cannot drift apart.
    description: landingCopy.zh.hero.sub,
    title: 'ServerBee：自托管的 VPS 监控，细到每一条线路'
  }
}
