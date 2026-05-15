import { LightBandArrow } from '../animations/light-band'
import { GradientHeading } from '../primitives/gradient-heading'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function HowItWorks({ lang }: { lang: LandingLang }) {
  const copy = t(lang).how
  const steps = [copy.step1, copy.step2, copy.step3]
  return (
    <Section>
      <GradientHeading className="mb-12 max-w-2xl">{copy.title}</GradientHeading>
      <div className="flex flex-col items-stretch gap-6 md:flex-row md:items-center">
        {steps.map((s, i) => (
          <div className="flex flex-1 items-center gap-4" key={s.title}>
            <article className="flex-1 rounded-2xl border border-white/10 bg-white/[0.02] p-6">
              <div className="font-mono text-amber-300 text-xs">{`0${i + 1}`}</div>
              <h3 className="mt-2 font-semibold text-lg text-zinc-100">{s.title}</h3>
              <p className="mt-2 text-sm text-zinc-400 leading-relaxed">{s.body}</p>
            </article>
            {i < steps.length - 1 ? <LightBandArrow /> : null}
          </div>
        ))}
      </div>
    </Section>
  )
}
