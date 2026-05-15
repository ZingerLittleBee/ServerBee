import { DataStreamAnim } from '../animations/data-stream'
import { InstallBinaryAnim } from '../animations/install-binary'
import { OrbitIconsAnim } from '../animations/orbit-icons'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function Pillars({ lang }: { lang: LandingLang }) {
  const copy = t(lang).pillars
  const cards = [
    { ...copy.one, Anim: InstallBinaryAnim },
    { ...copy.two, Anim: DataStreamAnim },
    { ...copy.three, Anim: OrbitIconsAnim }
  ]
  return (
    <Section>
      <div className="grid gap-6 md:grid-cols-3">
        {cards.map(({ title, body, Anim }) => (
          <article
            className="group relative overflow-hidden rounded-2xl border border-white/10 bg-white/[0.02] p-6 transition hover:border-amber-400/30 hover:bg-white/[0.04]"
            key={title}
          >
            <Anim />
            <h3 className="mt-2 font-semibold text-lg text-zinc-100">{title}</h3>
            <p className="mt-2 text-sm text-zinc-400 leading-relaxed">{body}</p>
          </article>
        ))}
      </div>
    </Section>
  )
}
