import { Fragment, useState } from 'react'

import { MoreLink, Points } from '../chrome/copy-blocks'
import { typeset } from '../chrome/typeset'
import { Icon } from '../icon'
import { Flag } from '../mocks/flag'
import { GridLine, PlotLines } from '../mocks/plot'
import { buildNetwork, type NetView, resolveNetView } from '../model/network'
import { type LandingLang, landingCopy } from '../translations'

interface NetworkSectionProps {
  lang: LandingLang
  /** Shared clock for the live chart; 0 during SSR and the first client render. */
  tick: number
}

export function NetworkSection({ lang, tick }: NetworkSectionProps) {
  const copy = landingCopy[lang]
  const network = copy.network
  const p = copy.p
  // The view the visitor picked. Until they pick one, the panel alternates between both views.
  const [chosen, setChosen] = useState<NetView | null>(null)
  const view = resolveNetView(tick, chosen)
  const net = buildNetwork(lang, tick, view)
  // Focus or a pointer on either view button keeps the view on screen, so their pressed state only changes when
  // the visitor presses one.
  const freeze = () => {
    if (chosen === null) {
      setChosen(view)
    }
  }
  return (
    <section className="sec" id="network">
      <div className="shell feat">
        <div className="feat-copy">
          <h2 className="h2">{network.h2}</h2>
          <p className="lede">{typeset(network.lede)}</p>
          <Points className="points" points={network.points} />
          <MoreLink link={network.link} />
        </div>
        <div className="feat-visual">
          {/* Unlike the other mocks the panel itself stays exposed, because its two view buttons are real
              controls; everything else in it is decorative. */}
          <div className="panel np pm">
            <div aria-hidden="true" className="np-head">
              <Flag className="sv-flag" code={net.head.flag} />
              <span className="np-name">{net.head.name}</span>
              <span className="sv-pill on">{p.online}</span>
            </div>
            <div aria-hidden="true" className="np-meta">
              {net.head.facts.map((fact, index) => (
                <Fragment key={fact}>
                  {index > 0 ? ' · ' : null}
                  <span>{fact}</span>
                </Fragment>
              ))}
            </div>
            <div aria-hidden="true" className="np-tabs">
              {net.tabs.map((tab) => (
                <span className={tab.className} key={tab.id}>
                  <Icon name={tab.icon} />
                  {tab.label}
                </span>
              ))}
            </div>
            <div aria-hidden="true" className="np-bar">
              <span className="np-probe">{p.lastProbe}</span>
              <span className="np-acts">
                <span className="np-act">
                  <Icon name="route" />
                  {p.traceroute}
                </span>
                <span className="np-act">
                  <Icon name="sliders-horizontal" />
                  {p.manage}
                </span>
                <span className="np-act">
                  <Icon name="download" />
                  {p.csv}
                </span>
              </span>
            </div>
            <div aria-hidden="true" className="np-ranges">
              {net.ranges.map((range) => (
                <span className={range.className} key={range.id}>
                  {range.label}
                </span>
              ))}
            </div>
            {/* The fieldset names the two buttons as a group; .np-seg inside it keeps the segmented look. */}
            <fieldset className="np-views">
              <legend className="vh">{network.viewLabel}</legend>
              <div className="np-seg">
                <button
                  aria-pressed={net.isAll}
                  className={net.allClassName}
                  onClick={() => setChosen('all')}
                  onFocus={freeze}
                  onPointerEnter={freeze}
                  type="button"
                >
                  {p.allTargets}
                </button>
                <button
                  aria-pressed={net.isProv}
                  className={net.provClassName}
                  onClick={() => setChosen('prov')}
                  onFocus={freeze}
                  onPointerEnter={freeze}
                  type="button"
                >
                  {p.byProvider}
                </button>
              </div>
            </fieldset>
            <div aria-hidden="true" className="np-view">
              <div className="np-swap">
                {net.isAll ? (
                  <div className="np-chips">
                    {net.chips.map((chip) => (
                      <div className="np-chip" key={chip.id}>
                        <i className="np-dot" style={chip.dotStyle} />
                        <div className="np-cn">
                          <b>{chip.name}</b>
                          <span className="np-st">
                            <span className="np-lat">{chip.lat}</span>
                            <i className="np-sep">|</i>
                            <span className="np-loss">
                              <span className="np-ll">{p.lossRate}</span> {chip.loss}
                            </span>
                          </span>
                        </div>
                        <Icon className="np-eye" name="eye" />
                      </div>
                    ))}
                  </div>
                ) : null}
                {net.isProv ? (
                  <div className="np-prov">
                    {net.provs.map((provider) => (
                      <div className="pv" key={provider.id}>
                        <div className="pv-name">{provider.name}</div>
                        <div className="pv-avg">
                          {provider.avg}
                          <small>ms</small>
                        </div>
                        <div className="pv-loss">
                          <span className="pv-ll">{p.lossRate}</span> {provider.loss}
                        </div>
                        <div className="pv-rows">
                          {provider.rows.map((row) => (
                            <div className="pv-row" key={row.id}>
                              <i className="np-dot" style={row.dotStyle} />
                              <span>{row.name}</span>
                              <b>{row.lat}</b>
                            </div>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
              <div className="np-chart">
                <div className="np-ct">{p.latencyTitle}</div>
                <div className="plot">
                  {net.grid.map((line) => (
                    <GridLine key={line.label} label={line.label} top={line.top} />
                  ))}
                  <GridLine base top="100%" />
                  <PlotLines lines={net.lines} />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
