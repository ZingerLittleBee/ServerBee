import { typeset } from '../chrome/typeset'
import { type LandingLang, landingCopy } from '../translations'

/** Native <details>, so answers open without JavaScript; the first one starts open. */
export function FaqSection({ lang }: { lang: LandingLang }) {
  return (
    <section className="sec" id="faq">
      <div className="shell faq-wrap">
        <h2 className="h2">{landingCopy[lang].faq.h2}</h2>
        <div className="faq-list">
          {landingCopy[lang].faq.items.map((item, index) => (
            <details className="faq-item" key={item.q} open={index === 0}>
              <summary>{item.q}</summary>
              <p className="faq-a">{typeset(item.a)}</p>
            </details>
          ))}
        </div>
      </div>
    </section>
  )
}
