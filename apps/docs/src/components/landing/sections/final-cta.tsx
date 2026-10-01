import { LandingLink } from '../chrome/landing-link'
import { typeset } from '../chrome/typeset'
import { Icon } from '../icon'
import { docsPath, type LandingLang, landingCopy, landingLinks } from '../translations'

export function FinalCtaSection({ lang }: { lang: LandingLang }) {
  const final = landingCopy[lang].final
  return (
    <section className="sec sec-final">
      <div className="shell">
        <div className="final">
          <img alt="" className="final-mark" height={56} src="/logo-icon.svg" width={56} />
          <h2 className="h2">{final.h2}</h2>
          <p className="final-sub">{typeset(final.sub)}</p>
          <div className="cta-row">
            <LandingLink className="btn btn-primary" href={docsPath(lang, 'quick-start')}>
              {final.cta1}
            </LandingLink>
            <a className="btn btn-ghost" href={landingLinks.github}>
              <Icon name="github" />
              {final.cta2}
            </a>
          </div>
        </div>
      </div>
    </section>
  )
}
