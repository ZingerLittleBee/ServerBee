import { Points, SectionHead } from '../chrome/copy-blocks'
import { CommandSegments } from '../mocks/command'
import { buildSecurity } from '../model/security'
import { type LandingLang, landingCopy } from '../translations'

export function SecuritySection({ lang }: { lang: LandingLang }) {
  const copy = landingCopy[lang]
  const security = copy.security
  const p = copy.p
  const model = buildSecurity(lang)
  return (
    <section className="sec alt" id="security">
      <div className="shell">
        <SectionHead copy={security} />
        <div className="sec-grid">
          <div aria-hidden="true" className="panel cap pm">
            <div className="cap-head">
              <span className="cap-title">{p.capsTitle}</span>
              <span className="cap-srv">{model.server}</span>
            </div>
            <div className="cap-desc">{p.capsDesc}</div>
            <div className="cap-grp">
              <div className="cap-gt">{p.capHigh}</div>
              <div className="cap-gd">{p.capHighDesc}</div>
              <div className="cap-rows">
                {model.capHigh.map((cap) => (
                  <div className="cap-row" key={cap.id}>
                    <span className="cap-name">{cap.name}</span>
                    {cap.tmp ? <span className="cap-tmp">{p.temporary}</span> : null}
                    <span className="cap-risk">{p.riskHigh}</span>
                    <span className={cap.stClassName}>{cap.st}</span>
                  </div>
                ))}
              </div>
            </div>
            <div className="cap-grp">
              <div className="cap-gt">{p.capLow}</div>
              <div className="cap-gd">{p.capLowDesc}</div>
              <div className="cap-chips">
                {model.capLow.map((cap) => (
                  <span className="cap-chip" key={cap.id}>
                    {cap.name}
                    <small>{cap.risk}</small>
                  </span>
                ))}
              </div>
            </div>
          </div>
          <div className="sec-side">
            <div className="term">
              <div className="term-bar">
                <i />
                <i />
                <i />
                <span>ops@hk-cn2: ~</span>
              </div>
              <div className="term-body">
                <div className="term-c">{security.termComment}</div>
                <div>
                  <span className="term-p">$</span>
                  <CommandSegments segments={model.grantSegments} />
                  <span className="term-cur" />
                </div>
              </div>
            </div>
            <Points className="sec-pts" points={security.points} />
          </div>
        </div>
      </div>
    </section>
  )
}
