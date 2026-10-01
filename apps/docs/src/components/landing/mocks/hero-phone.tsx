import { Fragment } from 'react'

import { Icon } from '../icon'
import { mockServer } from '../mock-data'
import { iosSpecs } from '../model/fleet'
import type { HeroModel } from '../model/hero'
import { type LandingLang, landingCopy } from '../translations'
import { Phone } from './phone'
import { GridLine, PlotLines } from './plot'

interface HeroPhoneProps {
  lang: LandingLang
  /** Latency lines and per-carrier target rows. */
  phoneNet: HeroModel['phoneNet']
  /** Detail segments before "More", with Network selected. */
  segNet: HeroModel['segNet']
  segNetMore: string
}

/** The server whose detail screen the hero phone shows. */
const server = mockServer('hk')

/** The small hero phone: the HK · CN2 GIA detail screen on its Network segment. */
export function HeroPhone({ lang, phoneNet, segNet, segNetMore }: HeroPhoneProps) {
  const ios = landingCopy[lang].p.ios
  return (
    <Phone lang={lang} size="sm">
      <div className="ios-navbar inl">
        <span className="ios-back">
          <Icon name="chevron-left" />
          {ios.back}
        </span>
        <span className="ios-navt">{server.name}</span>
        <Icon className="ios-plus" name="ellipsis" />
      </div>
      <div className="ios-chips">
        <span className="ios-chip on">{ios.chipOnline}</span>
        <span className="ios-chip">{server.os}</span>
        <span className="ios-chip">{iosSpecs(server)}</span>
      </div>
      <div className="ios-seg">
        {segNet.map((seg) => (
          <span className={seg.className} key={seg.id}>
            {seg.label}
          </span>
        ))}
        <span>{segNetMore}</span>
      </div>
      <div className="ios-seg sm">
        <span className="on">1h</span>
        <span>6h</span>
        <span>24h</span>
        <span>7d</span>
      </div>
      <div className="ios-card">
        <div className="ios-cap">{ios.probeHealth}</div>
        <div className="ios-health">
          <b>{ios.healthy}</b>
          <span>
            {ios.targetsN} · {ios.lastProbe}
          </span>
        </div>
      </div>
      <div className="ios-card">
        <div className="ios-cap">{ios.latency}</div>
        <div className="ios-plot">
          <GridLine label="100" top="0%" />
          <GridLine label="50" top="50%" />
          <GridLine base label="0" top="100%" />
          <PlotLines lines={phoneNet.lines} />
        </div>
      </div>
      <div className="ios-card">
        <div className="ios-cap">{ios.targets}</div>
        {phoneNet.groups.map((group) => (
          <Fragment key={group.id}>
            <div className="ios-grp">{group.name}</div>
            {group.rows.map((row) => (
              <div className="ios-trow" key={row.id}>
                <i style={row.dotStyle} />
                <span>{row.name}</span>
                <b>{row.lat}</b>
                <small>{row.loss}</small>
              </div>
            ))}
          </Fragment>
        ))}
      </div>
    </Phone>
  )
}
