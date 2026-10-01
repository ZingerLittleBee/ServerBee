import { Icon } from '../icon'
import type { ServerCard, StripCell } from '../model/fleet'
import type { NavItem } from '../model/hero'
import { type LandingLang, landingCopy, type ProductCopy } from '../translations'
import { Flag } from './flag'

interface HeroWindowProps {
  /** The first four server cards of the shared fleet. */
  cards: readonly ServerCard[]
  lang: LandingLang
  sideNav: readonly NavItem[]
}

/** The web panel's Servers page: icon sidebar, top bar and a two-column grid of server cards. */
export function HeroWindow({ cards, lang, sideNav }: HeroWindowProps) {
  const p = landingCopy[lang].p
  return (
    <div className="pw pm">
      <aside className="pw-side">
        <div className="pw-logo">
          <img alt="" height={20} src="/logo-icon.svg" width={20} />
          <span>ServerBee</span>
        </div>
        {sideNav.map((item) => (
          <div className={item.className} key={item.id}>
            <Icon name={item.icon} />
            <span>{item.label}</span>
          </div>
        ))}
      </aside>
      <div className="pw-main">
        <div className="pw-top">
          <Icon name="panel-left" />
          <span className="pw-title">{p.serversTitle}</span>
          <span className="pw-search">
            <Icon name="search" />
            {p.search}
          </span>
          <span className="pw-add">
            <Icon name="plus" />
            {p.addServer}
          </span>
        </div>
        <div className="pw-body">
          {cards.map((card) => (
            <ServerCardView card={card} key={card.id} p={p} />
          ))}
        </div>
      </div>
    </div>
  )
}

/** One server card: dials, network and disk rates, then the latency and loss strips. */
function ServerCardView({ card, p }: { card: ServerCard; p: ProductCopy }) {
  return (
    <div className={card.className}>
      <div className="sv-head">
        <Flag className="sv-flag" code={card.flag} />
        <span className="sv-name">{card.name}</span>
        <span className={card.pillClassName}>{card.status}</span>
      </div>
      <div className="sv-rings">
        {card.rings.map((ring) => (
          <div className="sv-ring" key={ring.id}>
            <div className="sv-dial" style={ring.style}>
              <b>{ring.val}</b>
            </div>
            <div className="sv-rl">
              <b>{ring.label}</b>
              <span>{ring.sub}</span>
            </div>
          </div>
        ))}
      </div>
      <div className="sv-net">
        {card.net.map((rate) => (
          <div key={rate.id}>
            <div className="sv-nk">
              <b>{rate.k}</b>
              <span>{rate.k2}</span>
            </div>
            <div className="sv-nv">
              {rate.v}
              <small>{rate.u}</small>
            </div>
          </div>
        ))}
      </div>
      <div className="sv-strips">
        <div>
          <div className="sv-sh">
            <span>{p.latency}</span>
            <span className={card.latClassName}>
              {card.latTxt}
              <small>ms</small>
            </span>
          </div>
          <Strip cells={card.latCells} />
        </div>
        <div>
          <div className="sv-sh">
            <span>{p.loss}</span>
            <span className={card.lossClassName}>{card.lossTxt}</span>
          </div>
          <Strip cells={card.lossCells} />
        </div>
      </div>
    </div>
  )
}

/** A status strip; the cells are shapes (`<i class="cell …">`), not icons. */
function Strip({ cells }: { cells: readonly StripCell[] }) {
  return (
    <div className="strip">
      {cells.map((cell) => (
        <i className={cell.className} key={cell.id} />
      ))}
    </div>
  )
}
