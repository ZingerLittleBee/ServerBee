import { Hero } from './sections/hero'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
    </div>
  )
}
