/* Search and social copy for the landing page. The route head in routes/$lang/index.tsx turns it into tags. */

import { type LandingLang, landingCopy } from './translations'

export interface LandingSeoCopy {
  /** The meta description, also used for Open Graph and Twitter cards. */
  description: string
  /** Alt text of the share image, public/og/landing-{lang}.png. */
  imageAlt: string
  title: string
}

export const landingSeo: Record<LandingLang, LandingSeoCopy> = {
  en: {
    description:
      'Self-hosted VPS monitoring with live metrics, network quality, IP reputation, alerts and costs in one panel, plus drag-and-drop dashboards and a native iOS app.',
    imageAlt:
      'ServerBee: Self-hosted VPS monitoring, down to every route. A radar shows each server’s latency to Shanghai.',
    title: 'ServerBee: self-hosted VPS monitoring, down to every route'
  },
  zh: {
    // The hero subtitle doubles as the description, so the two cannot drift apart.
    description: landingCopy.zh.hero.sub,
    imageAlt: 'ServerBee：自托管的 VPS 监控，细到每一条线路。雷达图显示各服务器到上海的延迟。',
    title: 'ServerBee：自托管的 VPS 监控，细到每一条线路'
  }
}
