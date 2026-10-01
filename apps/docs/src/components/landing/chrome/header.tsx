import { Link } from '@tanstack/react-router'
import { useTheme } from 'next-themes'

import { Icon } from '../icon'
import { docsPath, type LandingLang, landingCopy, landingLinks } from '../translations'
import { LandingLink } from './landing-link'

export function Header({ lang }: { lang: LandingLang }) {
  const nav = landingCopy[lang].nav
  const otherLang: LandingLang = lang === 'zh' ? 'en' : 'zh'
  const { resolvedTheme, setTheme } = useTheme()

  return (
    <header className="nav">
      <div className="shell nav-in">
        <a className="brand" href="#top">
          <img alt="" className="brand-mark" height={28} src="/logo-icon.svg" width={28} />
          <span className="brand-name">ServerBee</span>
        </a>
        <nav aria-label={nav.label} className="nav-links">
          {nav.links.map((link) => (
            <LandingLink href={link.href} key={link.label}>
              {link.label}
            </LandingLink>
          ))}
        </nav>
        <div className="nav-act">
          {/* The label stays when narrow screens hide the text. */}
          <a aria-label="GitHub" className="gh-link" href={landingLinks.github}>
            <Icon name="github" />
            <span className="gh-text">GitHub</span>
          </a>
          <Link
            aria-label={nav.langLabel}
            className="txt-btn"
            hrefLang={otherLang}
            lang={otherLang}
            params={{ lang: otherLang }}
            to="/$lang"
          >
            {nav.langBtn}
          </Link>
          {/* resolvedTheme is unknown until hydration, so both icons render and CSS shows the right one. */}
          <button
            aria-label={nav.themeLabel}
            className="icon-btn"
            onClick={() => setTheme(resolvedTheme === 'dark' ? 'light' : 'dark')}
            type="button"
          >
            <Icon className="th-sun" name="sun" />
            <Icon className="th-moon" name="moon" />
          </button>
          <LandingLink className="btn btn-primary btn-sm nav-cta" href={docsPath(lang, 'quick-start')}>
            {nav.cta}
          </LandingLink>
        </div>
      </div>
    </header>
  )
}
