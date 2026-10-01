import { typeset } from '../chrome/typeset'
import { type LandingLang, landingCopy } from '../translations'

export function MoreSection({ lang }: { lang: LandingLang }) {
  const more = landingCopy[lang].more
  return (
    <section className="sec" id="more">
      <div className="shell">
        <h2 className="h2">{more.h2}</h2>
        <ul className="more-grid">
          {more.items.map((item) => (
            <li key={item.t}>
              <span className="more-t">{item.t}</span>
              <span className="more-d">{typeset(item.d)}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  )
}
