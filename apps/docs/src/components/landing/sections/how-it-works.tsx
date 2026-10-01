import { useSyncExternalStore } from 'react'

import { LandingLink } from '../chrome/landing-link'
import { typeset } from '../chrome/typeset'
import { CommandSegments } from '../mocks/command'
import { buildHowItWorks } from '../model/how-it-works'
import { type LandingLang, landingCopy } from '../translations'

/** The id of the n-th note (1-based), which the n-th stat's mark links to. */
const noteId = (n: number) => `how-note-${n}`

function subscribeHash(onChange: () => void): () => void {
  window.addEventListener('hashchange', onChange)
  window.addEventListener('popstate', onChange)
  return () => {
    window.removeEventListener('hashchange', onChange)
    window.removeEventListener('popstate', onChange)
  }
}

/** The URL fragment without its '#', percent-decoded as the browser does when it looks for the target element. */
function readHash(): string {
  const raw = window.location.hash.slice(1)
  try {
    return decodeURIComponent(raw)
  } catch {
    return raw
  }
}

/**
 * The URL fragment, empty on the server and while hydrating, so hydration matches.
 * The jumped-to note is highlighted from this rather than with :target, which WebKit leaves on the last note after
 * Back once the router has set history.scrollRestoration to manual.
 */
function useHash(): string {
  return useSyncExternalStore(subscribeHash, readHash, () => '')
}

export function HowItWorksSection({ lang }: { lang: LandingLang }) {
  const how = landingCopy[lang].how
  const diagram = how.diagram
  const model = buildHowItWorks(lang)
  const hash = useHash()
  return (
    <section className="sec alt" id="how">
      <div className="shell">
        <h2 className="h2">{how.h2}</h2>
        <div className="arch">
          <div className="arch-box">
            <div className="arch-cap">{diagram.agents}</div>
            <div className="arch-title mono">{diagram.agentBin}</div>
            <ul className="arch-list">
              {model.archAgents.map((agent) => (
                <li key={agent.id}>
                  <span>{agent.name}</span>
                  <small>{agent.os}</small>
                </li>
              ))}
            </ul>
          </div>
          <div className="arch-wire">
            <span>{diagram.wire}</span>
            <small>{typeset(diagram.wireNote)}</small>
          </div>
          <div className="arch-box arch-srv">
            <div className="arch-cap">{diagram.serverNote}</div>
            <div className="arch-title">{diagram.server}</div>
            <ul className="arch-list">
              {diagram.serverRows.map((row) => (
                <li key={row}>{row}</li>
              ))}
            </ul>
          </div>
          <div className="arch-wire short" />
          <div className="arch-clients">
            {diagram.clients.map((client) => (
              <div className="arch-client" key={client.t}>
                <b>{client.t}</b>
                <span>{typeset(client.d)}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="steps">
          {model.steps.map((step) => (
            <div className="step" key={step.id}>
              <span className="step-n">{step.n}</span>
              <h3 className="step-t">{step.t}</h3>
              <p className="step-d">{typeset(step.d)}</p>
              <pre className="step-cmd">
                <CommandSegments segments={step.segments} />
              </pre>
            </div>
          ))}
        </div>
        {/* Each stat's mark is its position, matching the browser's numbering of the notes below, and links to that
            note. */}
        <ul className="stats">
          {how.stats.map((stat, index) => (
            <li key={stat.l}>
              <div className="stat-v">
                {stat.v}
                <sup>
                  <a aria-label={`${how.noteRef} ${index + 1}`} href={`#${noteId(index + 1)}`}>
                    {index + 1}
                  </a>
                </sup>
              </div>
              <div className="stat-l">{typeset(stat.l)}</div>
            </li>
          ))}
        </ul>
        <p className="notes-t">{how.notesTitle}</p>
        <ol className="notes">
          {how.notes.map((note, index) => {
            const id = noteId(index + 1)
            const className = hash === id ? 'on' : undefined
            return typeof note === 'string' ? (
              <li className={className} id={id} key={note}>
                {typeset(note)}
              </li>
            ) : (
              <li className={className} id={id} key={note.text}>
                {typeset(note.text)}
                <LandingLink href={note.link.href}>{note.link.label}</LandingLink>
                {note.after}
              </li>
            )
          })}
        </ol>
      </div>
    </section>
  )
}
