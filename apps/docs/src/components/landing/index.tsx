import { Hero } from './sections/hero'
import { Pillars } from './sections/pillars'
import { TrustStrip } from './sections/trust-strip'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
      <TrustStrip lang={lang} />
      <Pillars lang={lang} />
    </div>
  )
}
