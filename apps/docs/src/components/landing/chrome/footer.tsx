import { type LandingLang, landingCopy } from '../translations'
import { LandingLink } from './landing-link'
import { typeset } from './typeset'

export function Footer({ lang }: { lang: LandingLang }) {
  const footer = landingCopy[lang].footer
  return (
    <footer className="foot">
      <div className="shell">
        <div className="foot-in">
          <div className="foot-brand">
            <img alt="" className="brand-mark" height={28} src="/logo-icon.svg" width={28} />
            ServerBee
            <span>{typeset(footer.tagline)}</span>
          </div>
          <nav aria-label={footer.linksLabel} className="foot-links">
            {footer.links.map((link) => (
              <LandingLink href={link.href} key={link.label}>
                {link.label}
              </LandingLink>
            ))}
          </nav>
        </div>
        <p className="foot-legal">{typeset(footer.legal)}</p>
      </div>
    </footer>
  )
}
