import { Fragment } from 'react'

import { Points, SectionHead } from '../chrome/copy-blocks'
import { buildIpQuality } from '../model/ip-quality'
import { type LandingLang, landingCopy } from '../translations'

export function IpQualitySection({ lang }: { lang: LandingLang }) {
  const copy = landingCopy[lang]
  const ipq = copy.ipq
  const p = copy.p
  const model = buildIpQuality(lang)
  return (
    <section className="sec" id="ipq">
      <div className="shell">
        <SectionHead copy={ipq} />
        <div aria-hidden="true" className="panel ipp pm">
          <div className="ipp-title">{p.ipTitle}</div>
          <div className="ipp-desc">{p.ipDesc}</div>
          <div className="ipc-grid">
            {model.cards.map((card) => (
              <div className="ipc" key={card.id}>
                <div className="ipc-top">
                  <span className="ipc-name">{card.name}</span>
                  <span className={card.riskClassName}>{card.risk}</span>
                </div>
                <div className="ipc-ip">
                  <span>{card.ip}</span>
                  <span className="ipc-type">{card.type}</span>
                </div>
                <div className="ipc-flags">
                  {card.flags.map((flag) => (
                    <span className="ipc-flag" key={flag.id}>
                      {flag.label}
                    </span>
                  ))}
                </div>
                <div className="ipc-row">
                  <span>{p.location}</span>
                  <b>{card.loc}</b>
                </div>
                <div className="ipc-row">
                  <span>{p.checked}</span>
                  <b>{p.checkedAt}</b>
                </div>
              </div>
            ))}
          </div>
          <div className="mx-title">{p.matrix}</div>
          {/* Chrome makes an overflowing scroller a Tab stop; this one sits inside an aria-hidden mock. */}
          <div className="mx-wrap" tabIndex={-1}>
            {/* One flat grid: CSS finds the last row through :nth-last-child, so rows add no wrapper. */}
            <div className="mx">
              <div className="mx-corner mx-srv">{p.server}</div>
              <div className="end mx-grp" style={{ gridColumn: 'span 5' }}>
                {model.catStreaming}
              </div>
              <div className="end mx-grp" style={{ gridColumn: 'span 2' }}>
                AI
              </div>
              <div className="mx-grp" style={{ gridColumn: 'span 2' }}>
                {model.catSocial}
              </div>
              {model.services.map((service) => (
                <div className={service.className} key={service.id}>
                  {service.name}
                </div>
              ))}
              {model.unlock.map((row) => (
                <Fragment key={row.id}>
                  <div className="mx-srv">{row.name}</div>
                  {row.cells.map((cell) => (
                    <div className={cell.cellClassName} key={cell.id}>
                      <span className={cell.className}>{cell.label}</span>
                    </div>
                  ))}
                </Fragment>
              ))}
            </div>
          </div>
        </div>
        <Points className="pts-row four" points={ipq.points} />
      </div>
    </section>
  )
}
