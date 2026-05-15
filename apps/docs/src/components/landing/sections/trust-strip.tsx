import { Cpu, Gauge, PackageCheck } from 'lucide-react'

import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function TrustStrip({ lang }: { lang: LandingLang }) {
  const copy = t(lang).trust
  const items = [
    { Icon: PackageCheck, label: copy.binary },
    { Icon: Gauge, label: copy.realtime },
    { Icon: Cpu, label: copy.deps }
  ]
  return (
    <Section className="!py-10 border-white/5 border-y bg-white/[0.015]">
      <ul className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        {items.map(({ Icon, label }) => (
          <li
            className="flex items-center gap-3 rounded-xl border border-white/5 bg-white/[0.02] px-4 py-3"
            key={label}
          >
            <Icon aria-hidden className="h-5 w-5 text-amber-400" />
            <span className="text-sm text-zinc-300">{label}</span>
          </li>
        ))}
      </ul>
    </Section>
  )
}
