import type { LandingLang } from './translations'
import { t } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  const copy = t(lang)
  return (
    <div className="serverbee-landing dark min-h-screen" style={{ colorScheme: 'dark' }}>
      <main className="mx-auto w-full max-w-6xl px-6 py-24">
        <h1 className="gradient-text font-semibold text-5xl tracking-tight">{copy.hero.headline1}</h1>
        <p className="mt-4 text-zinc-400">{copy.hero.sub}</p>
      </main>
    </div>
  )
}
