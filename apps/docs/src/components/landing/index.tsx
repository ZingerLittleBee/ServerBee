import { type ReactElement, useMemo, useState } from 'react'

import { Footer } from './chrome/footer'
import { Header } from './chrome/header'
import { DashboardsSection } from './sections/dashboards'
import { FaqSection } from './sections/faq'
import { FinalCtaSection } from './sections/final-cta'
import { HeroSection } from './sections/hero'
import { HowItWorksSection } from './sections/how-it-works'
import { IosSection } from './sections/ios'
import { IpQualitySection } from './sections/ip-quality'
import { MoreSection } from './sections/more'
import { NetworkSection } from './sections/network'
import { SecuritySection } from './sections/security'
import type { LandingLang, SectionKey } from './translations'
import { useReducedMotion, useTick } from './use-tick'

/** DOM order per locale: Chinese readers come for network quality, English readers for dashboards. */
const sectionOrder: Record<LandingLang, SectionKey[]> = {
  zh: ['hero', 'network', 'ipq', 'security', 'dash', 'ios', 'more', 'how', 'faq', 'final'],
  en: ['hero', 'dash', 'network', 'ipq', 'security', 'ios', 'more', 'how', 'faq', 'final']
}

export function LandingPage({ lang }: { lang: LandingLang }) {
  const reducedMotion = useReducedMotion()
  // The visitor's choice from the hero's live toggle. Until they press it, the live mocks run unless the
  // system asks for reduced motion; pressing Resume then runs them anyway.
  const [liveChoice, setLiveChoice] = useState<boolean | null>(null)
  const live = liveChoice ?? !reducedMotion
  const tick = useTick(live)

  // Parts without live data keep the same element between ticks, so React skips re-rendering them.
  const still = useMemo(
    () => ({
      header: <Header lang={lang} />,
      footer: <Footer lang={lang} />,
      ipq: <IpQualitySection key="ipq" lang={lang} />,
      security: <SecuritySection key="security" lang={lang} />,
      more: <MoreSection key="more" lang={lang} />,
      how: <HowItWorksSection key="how" lang={lang} />,
      faq: <FaqSection key="faq" lang={lang} />,
      final: <FinalCtaSection key="final" lang={lang} />
    }),
    [lang]
  )

  const sections: Record<SectionKey, ReactElement> = {
    hero: <HeroSection key="hero" lang={lang} live={live} onToggleLive={() => setLiveChoice(!live)} tick={tick} />,
    network: <NetworkSection key="network" lang={lang} tick={tick} />,
    dash: <DashboardsSection key="dash" lang={lang} tick={tick} />,
    ios: <IosSection key="ios" lang={lang} tick={tick} />,
    ipq: still.ipq,
    security: still.security,
    more: still.more,
    how: still.how,
    faq: still.faq,
    final: still.final
  }

  // Tailwind utilities and fumadocs components do not apply inside .sb: the isolation rule at the top of
  // landing.css reverts them. Style landing markup with scoped .sb classes and render fumadocs pieces outside
  // this root. While the live preview is stopped, data-motion also pauses the landing's CSS animations.
  return (
    <div className={`sb l-${lang}`} data-motion={live ? undefined : 'paused'}>
      {still.header}
      <main className="page">{sectionOrder[lang].map((key) => sections[key])}</main>
      {still.footer}
    </div>
  )
}
