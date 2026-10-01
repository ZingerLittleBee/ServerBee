import { Fragment } from 'react'

import { MoreLink, Points } from '../chrome/copy-blocks'
import { typeset } from '../chrome/typeset'
import { Icon } from '../icon'
import { Phone } from '../mocks/phone'
import { buildIos, type IosModel } from '../model/ios'
import { type LandingLang, landingCopy, type ProductCopy } from '../translations'

interface IosSectionProps {
  lang: LandingLang
  /** Shared clock for the live server list; 0 during SSR and the first client render. */
  tick: number
}

interface ScreenProps {
  model: IosModel
  p: ProductCopy
}

export function IosSection({ lang, tick }: IosSectionProps) {
  const c = landingCopy[lang]
  const ios = c.ios
  const model = buildIos(lang, tick)
  return (
    <section className="sec" id="ios">
      <div className="shell feat">
        <div className="feat-copy">
          <h2 className="h2">{ios.h2}</h2>
          <AvailabilityChip text={ios.chip} />
          <p className="lede">{typeset(ios.lede)}</p>
          <Points className="points" points={ios.points} />
          <MoreLink link={ios.link} />
        </div>
        <div aria-hidden="true" className="feat-visual">
          <div className="phones">
            <Phone lang={lang} overlay={<PushBanner model={model} p={c.p} />} size="md">
              <ServerListScreen model={model} p={c.p} />
            </Phone>
            <Phone lang={lang} size="md">
              <IpQualityScreen model={model} p={c.p} />
            </Phone>
          </div>
        </div>
      </div>
    </section>
  )
}

/**
 * The availability chip. When its copy does not fit on one line (the narrowest phones), it wraps only at
 * the " · " separators: every clause stays whole and each separator stays with the clause before it.
 * The single wrapper span keeps `.avail` (inline flex with a gap) at one flex item, so one-line
 * rendering is unchanged.
 */
function AvailabilityChip({ text }: { text: string }) {
  return (
    <span className="avail">
      <span>
        {text.split(' · ').map((clause, index) => (
          <Fragment key={clause}>
            {index > 0 ? '\u00a0· ' : null}
            <span className="avail-clause">{clause}</span>
          </Fragment>
        ))}
      </span>
    </span>
  )
}

/** The alert notification over the server list (FRA · Storage crossed its CPU rule). */
function PushBanner({ model, p }: ScreenProps) {
  return (
    <div className="ios-push">
      <img alt="" height={38} src="/logo-icon.svg" width={38} />
      <div className="ios-pm">
        <div className="ios-pt">
          <span>{model.pushTitle}</span>
          <small>{p.ios.pushNow}</small>
        </div>
        <div className="ios-pb">{model.pushBody}</div>
      </div>
    </div>
  )
}

/** Servers tab: fleet tiles, then every server with its live CPU, memory and disk bars. */
function ServerListScreen({ model, p }: ScreenProps) {
  const ios = p.ios
  return (
    <>
      <div className="ios-navbar">
        <span />
        <Icon className="ios-plus" name="plus" />
      </div>
      <div className="ios-large">{p.serversTitle}</div>
      <div className="ios-search">
        <Icon name="search" />
        {ios.search}
      </div>
      <div className="ios-tiles">
        <div className="ios-tile">
          <span>{ios.online}</span>
          <b>
            {model.onlineCount}
            <small>{`/ ${model.serverCount}`}</small>
          </b>
        </div>
        <div className="ios-tile">
          <span>{ios.alerts}</span>
          <b className="warn">{model.alertCount}</b>
        </div>
        <div className="ios-tile">
          <span>{ios.trafficDown}</span>
          <b>
            {model.fleetDownV}
            <small>MB/s</small>
          </b>
        </div>
      </div>
      <div className="ios-list">
        {model.phoneRows.map((row) => (
          <div className="ios-row" key={row.id}>
            <i className={row.dotClassName} />
            <div className="ios-rm">
              <div className="ios-rt">
                <b>{row.name}</b>
                <span className={row.trailClassName}>{row.trail}</span>
              </div>
              <div className="ios-det">{row.details}</div>
              {row.online ? (
                <div className="ios-bars">
                  {row.bars.map((bar) => (
                    <div key={bar.id}>
                      <div className="ios-bl">
                        <span>{bar.label}</span>
                        <span>{bar.val}</span>
                      </div>
                      <div className="ios-bt">
                        <i style={bar.style} />
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        ))}
      </div>
    </>
  )
}

/** TYO · BGP detail on its IP tab: the risk ring, IP type badges and service access. */
function IpQualityScreen({ model, p }: ScreenProps) {
  const ios = p.ios
  return (
    <>
      <div className="ios-navbar inl">
        <span className="ios-back">
          <Icon name="chevron-left" />
          {ios.back}
        </span>
        <span className="ios-navt">{model.ipServer.name}</span>
        <Icon className="ios-plus" name="ellipsis" />
      </div>
      <div className="ios-chips">
        <span className="ios-chip on">{model.chipOnlineTyo}</span>
        <span className="ios-chip">{model.ipServer.os}</span>
        <span className="ios-chip">{model.ipServer.specs}</span>
      </div>
      <div className="ios-seg">
        {model.segIp.map((segment) => (
          <span className={segment.className} key={segment.id}>
            {segment.label}
          </span>
        ))}
        <span className="on">
          IP
          <Icon name="chevron-down" />
        </span>
      </div>
      <div className="ios-card">
        <div className="ios-risk">
          <div className="ios-ring" style={model.ipRingStyle}>
            <b>{model.ipRisk}</b>
          </div>
          <div className="ios-rk">
            <b>{ios.riskMid}</b>
            <span>{ios.riskScore}</span>
          </div>
        </div>
        <div className="ios-det ios-ipline">{ios.ipLine}</div>
        <div className="ios-badges">
          <span className="ios-badge">{ios.dc}</span>
          <span className="ios-badge warn">{ios.hosting}</span>
        </div>
      </div>
      <div className="ios-card">
        <div className="ios-cap">{ios.access}</div>
        {model.phoneIp.map((group) => (
          <Fragment key={group.id}>
            <div className="ios-grp">{group.cat}</div>
            {group.rows.map((row) => (
              <div className="ios-arow" key={row.id}>
                <span>{row.name}</span>
                <small>{row.region}</small>
                <em className={row.className}>{row.st}</em>
              </div>
            ))}
          </Fragment>
        ))}
      </div>
      <div className="ios-btn">{ios.recheck}</div>
    </>
  )
}
