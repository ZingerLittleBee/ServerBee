import { useEffect, useRef, useState } from 'react'

import { LandingLink } from '../chrome/landing-link'
import { typeset } from '../chrome/typeset'
import { Icon } from '../icon'
import { CommandSegments } from '../mocks/command'
import { HeroPhone } from '../mocks/hero-phone'
import { HeroWindow } from '../mocks/hero-window'
import { buildHero, buildInstall, buildLiveToggle, type CopyState } from '../model/hero'
import { docsPath, type InstallTab, type LandingLang, landingCopy, landingLinks } from '../translations'

/** How long the copy button shows "Copied" or "Copy failed". */
const COPIED_MS = 1600

interface HeroSectionProps {
  lang: LandingLang
  /** Whether the live mocks are running; the toggle under the hero window flips it. */
  live: boolean
  onToggleLive: () => void
  /** Shared clock for the live mocks; 0 during SSR and the first client render. */
  tick: number
}

export function HeroSection({ lang, live, onToggleLive, tick }: HeroSectionProps) {
  const hero = landingCopy[lang].hero
  const visual = buildHero(lang, tick)
  const liveToggle = buildLiveToggle(lang, live)

  return (
    <section className="hero hero-split" id="top">
      <div className="shell hero-grid">
        <div className="hero-copy">
          <a className="release" href={landingLinks.releases}>
            <span className="release-dot" />
            <span>{hero.release}</span>
            <span className="release-link">
              {hero.releaseLink} <span aria-hidden="true">→</span>
            </span>
          </a>
          <h1 className="h1">
            <span>{hero.h1a}</span> <span>{hero.h1b}</span>
          </h1>
          <p className="hero-sub">{typeset(hero.sub)}</p>
          <div className="cta-row">
            <LandingLink className="btn btn-primary" href={docsPath(lang, 'quick-start')}>
              {hero.cta1}
            </LandingLink>
            <a className="btn btn-ghost" href={landingLinks.status}>
              <span>{hero.cta2}</span> <span aria-hidden="true">↗</span>
            </a>
          </div>
          <InstallBox lang={lang} />
          <ul className="facts">
            {hero.facts.map((fact) => (
              <li key={fact}>
                <span>{typeset(fact)}</span>
              </li>
            ))}
          </ul>
        </div>

        <div className="hero-visual">
          <div aria-hidden="true" className="hv">
            <div className="hv-rings" />
            <HeroWindow cards={visual.cards} lang={lang} sideNav={visual.sideNav} />
            <div className="hv-phone">
              <HeroPhone lang={lang} phoneNet={visual.phoneNet} segNet={visual.segNet} segNetMore={visual.segNetMore} />
            </div>
          </div>
          {/* Outside the aria-hidden mocks, so keyboard and screen reader users can stop them too (WCAG 2.2.2). */}
          <button className="live-btn" onClick={onToggleLive} type="button">
            <Icon name={liveToggle.icon} />
            <span>{liveToggle.label}</span>
          </button>
        </div>
      </div>
    </section>
  )
}

/** Selects the command in `pre`, after its "$" prompt, so it can be copied by hand. */
function selectCommand(pre: HTMLPreElement) {
  const selection = window.getSelection()
  if (!selection) {
    return
  }
  const range = document.createRange()
  range.selectNodeContents(pre)
  if (pre.firstChild) {
    range.setStartAfter(pre.firstChild)
  }
  selection.removeAllRanges()
  selection.addRange(range)
}

/** Install commands for Docker and the binary with a copy button, or the Railway template link. */
function InstallBox({ lang }: { lang: LandingLang }) {
  const hero = landingCopy[lang].hero
  const [tab, setTab] = useState<InstallTab>('docker')
  const [copyState, setCopyState] = useState<CopyState>('idle')
  const resetTimer = useRef<number | undefined>(undefined)
  // Bumped on every copy, tab change and unmount, so a clipboard write that settles late cannot mark
  // another tab's command as copied.
  const copyRun = useRef(0)
  // The selected tab's command; null on Railway.
  const command = useRef<HTMLPreElement>(null)
  const install = buildInstall(lang, tab, copyState)

  useEffect(
    () => () => {
      copyRun.current += 1
      window.clearTimeout(resetTimer.current)
    },
    []
  )

  const selectTab = (next: InstallTab) => {
    copyRun.current += 1
    window.clearTimeout(resetTimer.current)
    setTab(next)
    setCopyState('idle')
  }

  const copyCommand = async () => {
    copyRun.current += 1
    const run = copyRun.current
    let copied = true
    try {
      await navigator.clipboard.writeText(install.command)
    } catch {
      // No clipboard (insecure context, some in-app webviews) or a denied write: select the command instead.
      copied = false
    }
    if (run !== copyRun.current) {
      return
    }
    if (!copied && command.current) {
      selectCommand(command.current)
    }
    setCopyState(copied ? 'copied' : 'failed')
    window.clearTimeout(resetTimer.current)
    resetTimer.current = window.setTimeout(() => setCopyState('idle'), COPIED_MS)
  }

  return (
    <div className="install">
      <div className="ins-head">
        {install.tabs.map((item) => (
          <button
            aria-pressed={item.selected}
            className={item.className}
            key={item.id}
            onClick={() => selectTab(item.id)}
            type="button"
          >
            {item.label}
          </button>
        ))}
        {install.isRailway ? null : (
          <button className="ins-copy" onClick={copyCommand} type="button">
            <Icon name={install.copyIcon} />
            <span>{install.copyLabel}</span>
          </button>
        )}
      </div>
      <div className="ins-body">
        {install.panes.map((pane) =>
          pane.isRailway ? (
            <div className={pane.className} inert={!pane.selected} key={pane.id}>
              <span>{typeset(hero.railwayText)}</span>
              <a className="btn btn-ghost btn-sm" href={landingLinks.railway}>
                <span>{hero.railwayBtn}</span> <span aria-hidden="true">↗</span>
              </a>
            </div>
          ) : (
            <div className={pane.className} inert={!pane.selected} key={pane.id}>
              <pre className="ins-cmd" ref={pane.selected ? command : undefined}>
                <b>$</b>
                <CommandSegments segments={pane.segments} />
              </pre>
              <p className="ins-note">{typeset(hero.installNote)}</p>
            </div>
          )
        )}
      </div>
      {/* Announces the copy result (role status). The button label is not a live region: its reset stays quiet. */}
      <output className="vh">{install.copyStatus}</output>
    </div>
  )
}
