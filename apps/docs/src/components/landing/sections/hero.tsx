import { ArrowRight, Github } from 'lucide-react'

import { MiniDashboard } from '../animations/mini-dashboard'
import { CodeCopy } from '../primitives/code-copy'
import { GradientHeading } from '../primitives/gradient-heading'
import { HexBackground } from '../primitives/hex-background'
import { Section } from '../primitives/section'
import { INSTALL_COMMAND, type LandingLang, t } from '../translations'

export function Hero({ lang }: { lang: LandingLang }) {
  const copy = t(lang).hero
  const docsHref = `/${lang}/docs/quick-start`
  return (
    <Section className="overflow-hidden pt-28">
      <HexBackground />
      <div className="relative grid items-center gap-12 lg:grid-cols-[1.1fr_1fr]">
        <div>
          <span className="inline-flex items-center rounded-full border border-amber-400/30 bg-amber-400/10 px-3 py-1 text-amber-300 text-xs">
            {copy.eyebrow}
          </span>
          <GradientHeading as="h1" className="mt-5">
            {copy.headline1}
            <br />
            {copy.headline2}
          </GradientHeading>
          <p className="mt-6 max-w-xl text-base text-zinc-400 leading-relaxed sm:text-lg">{copy.sub}</p>

          <div className="mt-8 flex flex-wrap items-center gap-3">
            <a
              className="inline-flex items-center gap-2 rounded-lg bg-amber-400 px-5 py-2.5 font-medium text-amber-950 text-sm transition hover:bg-amber-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-300"
              href={docsHref}
            >
              {copy.primaryCta} <ArrowRight className="h-4 w-4" />
            </a>
            <a
              className="inline-flex items-center gap-2 rounded-lg border border-white/15 bg-white/[0.04] px-5 py-2.5 font-medium text-sm text-zinc-100 transition hover:bg-white/[0.08] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
              href="https://github.com/ZingerLittleBee/ServerBee"
              rel="noreferrer"
              target="_blank"
            >
              <Github className="h-4 w-4" /> {copy.secondaryCta}
            </a>
          </div>

          <div className="mt-6">
            <CodeCopy command={INSTALL_COMMAND} label={copy.installLabel} />
          </div>
        </div>

        <div className="relative flex justify-center lg:justify-end">
          <MiniDashboard />
        </div>
      </div>
    </Section>
  )
}
