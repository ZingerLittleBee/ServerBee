import { ArrowRight, Github } from 'lucide-react'

import { CodeCopy } from '../primitives/code-copy'
import { GradientHeading } from '../primitives/gradient-heading'
import { HexBackground } from '../primitives/hex-background'
import { Section } from '../primitives/section'
import { INSTALL_COMMAND, type LandingLang, t } from '../translations'

export function FinalCta({ lang }: { lang: LandingLang }) {
  const copy = t(lang).finalCta
  const docsHref = `/${lang}/docs/quick-start`
  return (
    <Section className="overflow-hidden">
      <HexBackground />
      <div className="relative mx-auto max-w-3xl text-center">
        <GradientHeading className="mx-auto">{copy.title}</GradientHeading>
        <p className="mx-auto mt-4 max-w-xl text-base text-zinc-400">{copy.sub}</p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <a
            className="inline-flex items-center gap-2 rounded-lg bg-amber-400 px-5 py-2.5 font-medium text-amber-950 text-sm transition hover:bg-amber-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-300"
            href={docsHref}
          >
            {copy.readDocs} <ArrowRight className="h-4 w-4" />
          </a>
          <a
            className="inline-flex items-center gap-2 rounded-lg border border-white/15 bg-white/[0.04] px-5 py-2.5 font-medium text-sm text-zinc-100 transition hover:bg-white/[0.08] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
            href="https://github.com/ZingerLittleBee/ServerBee"
            rel="noreferrer"
            target="_blank"
          >
            <Github className="h-4 w-4" /> {copy.star}
          </a>
        </div>
        <div className="mt-8 flex justify-center">
          <CodeCopy command={INSTALL_COMMAND} />
        </div>
      </div>
    </Section>
  )
}
