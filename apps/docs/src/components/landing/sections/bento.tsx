import type { ComponentType, ReactNode } from 'react'

import { AlertBellAnim } from '../animations/alert-bell'
import { ColorRingAnim } from '../animations/color-ring'
import { DockerStackAnim } from '../animations/docker-stack'
import { FileTreeAnim } from '../animations/file-tree'
import { MonitorDotsAnim } from '../animations/monitor-dots'
import { PingChartAnim } from '../animations/ping-chart'
import { TerminalDemoAnim } from '../animations/terminal-demo'
import { UpgradeLoopAnim } from '../animations/upgrade-loop'
import { GradientHeading } from '../primitives/gradient-heading'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

interface Tile {
  Anim: ComponentType
  body: string
  span: string
  title: string
}

export function Bento({ lang }: { lang: LandingLang }) {
  const copy = t(lang).bento
  const tiles: Tile[] = [
    { ...copy.network, Anim: PingChartAnim, span: 'md:col-span-6 md:row-span-2' },
    { ...copy.themes, Anim: ColorRingAnim, span: 'md:col-span-3' },
    { ...copy.alerts, Anim: AlertBellAnim, span: 'md:col-span-3' },
    { ...copy.monitors, Anim: MonitorDotsAnim, span: 'md:col-span-6' },
    { ...copy.terminal, Anim: TerminalDemoAnim, span: 'md:col-span-6 md:row-span-2' },
    { ...copy.file, Anim: FileTreeAnim, span: 'md:col-span-6' },
    { ...copy.docker, Anim: DockerStackAnim, span: 'md:col-span-3' },
    { ...copy.upgrade, Anim: UpgradeLoopAnim, span: 'md:col-span-3' }
  ]

  return (
    <Section>
      <GradientHeading className="mb-10 max-w-2xl">{bentoTitle(lang)}</GradientHeading>
      <div className="grid auto-rows-[240px] grid-cols-1 gap-4 md:grid-cols-12">
        {tiles.map(({ title, body, Anim, span }) => (
          <Card body={body} key={title} span={span} title={title}>
            <Anim />
          </Card>
        ))}
      </div>
    </Section>
  )
}

function Card({ title, body, span, children }: { title: string; body: string; span: string; children: ReactNode }) {
  return (
    <article
      className={`group flex flex-col gap-4 overflow-hidden rounded-2xl border border-white/10 bg-white/[0.02] p-5 transition hover:border-amber-400/30 hover:bg-white/[0.04] ${span}`}
    >
      <div className="min-h-0 flex-1">{children}</div>
      <div className="shrink-0">
        <h3 className="font-semibold text-base text-zinc-100">{title}</h3>
        <p className="landing-clamp-3 mt-1 text-sm text-zinc-400 leading-relaxed">{body}</p>
      </div>
    </article>
  )
}

function bentoTitle(lang: LandingLang): string {
  return lang === 'zh' ? '一个探针，覆盖运维的方方面面。' : 'One probe. Every job your VPS needs.'
}
