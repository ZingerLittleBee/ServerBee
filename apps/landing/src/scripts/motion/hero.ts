/*
 * Hero: routes leave the headline's full stop, run down the side of the page and into the panel, and each one
 * brings a server card with it. Afterwards packets keep travelling them, and a card rings when one arrives.
 */

import {
  EASE_OUT,
  flare,
  grow,
  overlay,
  rand,
  rectOf,
  reduced,
  regionsOf,
  root,
  sleep,
  svgEl,
  tile,
  watch,
  zh
} from './stage'

/** Space between two routes, and the radius of their corners, in pixels. */
const GAP = 7
const RADIUS = 9
/** Air between the hero's content and the first route that passes it. The room the stylesheet keeps includes it. */
const CLEAR = 14
/** How long the first route takes to draw, and how much longer each next one takes. */
const DRAW = 820
const DRAW_STEP = 70
/** When the routes leave, measured from the entrance: the headline's full stop is all but in place by then. */
const DEPART = 780
/** A carrier's usual latency in milliseconds, in the order of the tags. */
const LATENCY = [31, 28, 44]

interface Lane {
  packet: SVGPathElement
  plug: SVGCircleElement
  route: SVGPathElement
}

/** Where the headline's last line ends, in viewport pixels. `size` is its font size. */
function lastLineRight(heading: Element, size: number): number {
  // Pieces may be mid-entrance, which moves them vertically only, so their horizontal position can be trusted.
  const pieces = [...heading.querySelectorAll(':scope > span:last-child .piece')].map((piece) =>
    piece.getBoundingClientRect()
  )
  const lowest = Math.max(...pieces.map((rect) => rect.bottom))
  return Math.max(...pieces.filter((rect) => rect.bottom > lowest - 0.6 * size).map((rect) => rect.right))
}

export function mountHero(): void {
  const hero = document.querySelector<HTMLElement>('.hero')
  const heading = hero?.querySelector('h1')
  const figure = hero?.querySelector<HTMLElement>('.hero-shot .shot')
  const frame = hero?.querySelector<HTMLElement>('.hero-shot')
  const install = hero?.querySelector<HTMLElement>('.install')
  if (!(hero && heading && figure && frame && install)) {
    return
  }
  const cards = (regionsOf(figure).cards ?? []).map(rectOf)
  // The capture lays its cards out in rows; two routes reach each column, one for each of its first two cards.
  const columns = cards.filter((card) => card[1] === cards[0][1]).length
  if (columns === 0 || cards.length < 2 * columns) {
    return
  }
  const laneCount = 2 * columns
  const column = (lane: number): number => Math.floor(lane / 2)
  const cardOf = (lane: number): number => (lane % 2) * columns + column(lane)

  // The cards become tiles, so they can be dealt. A visitor who asked for less motion keeps the capture as it is.
  const between: [number, number] = [(cards[0][0] + cards[0][2] + cards[1][0]) / 2, cards[0][1] + 10]
  const fleet = reduced ? undefined : overlay(figure, 'fleet-layer', [between])
  const tiles = fleet ? cards.map((card) => tile(fleet.layer, grow(card, 0.42, fleet.aspect))) : []

  const svg = svgEl('svg', 'routes')
  svg.setAttribute('aria-hidden', 'true')
  const lanes: Lane[] = []
  for (let index = 0; index < laneCount; index++) {
    const route = svgEl('path', 'route')
    route.setAttribute('pathLength', '100')
    route.style.setProperty('--draw', `${DRAW + index * DRAW_STEP}ms`)
    const packet = svgEl('path', 'route-packet')
    packet.setAttribute('pathLength', '100')
    packet.dataset.lane = String(index)
    packet.style.setProperty('--period', `${Math.round(rand(2700, 4300))}ms`)
    // Packets go live when the first route is drawn. A route's own first packet waits until that route is drawn
    // too, and the first route's leaves at once.
    packet.style.setProperty('--delay', `${Math.round(index * DRAW_STEP + (index ? rand(0, 900) : 0))}ms`)
    const plug = svgEl('circle', 'route-plug')
    plug.setAttribute('r', '3')
    svg.append(route, packet, plug)
    lanes.push({ packet, plug, route })
  }
  hero.prepend(svg)

  const tags = [...hero.querySelectorAll<HTMLElement>('.route-tag')].map((tag, index) => {
    const value = tag.querySelector('b')
    if (value) {
      value.textContent = String(LATENCY[index % LATENCY.length])
    }
    return { tag, value }
  })

  /** Lays the routes out from the end of the headline to the panel. False when there is no room for them. */
  const layout = (): boolean => {
    const box = hero.getBoundingClientRect()
    const shot = figure.getBoundingClientRect()
    const size = Number.parseFloat(getComputedStyle(heading).fontSize)
    // The routes leave from the headline's closing full stop, which sits to the left of its box in Chinese. The
    // block gives the height: it does not move while its pieces arrive.
    const startX = lastLineRight(heading, size) - box.left + (zh ? -0.44 : 0.14) * size
    const startY = heading.getBoundingClientRect().bottom - box.top - (zh ? 0.325 : 0.3) * size
    // The routes go down beside the facts and the install block, never behind them: the stylesheet ends both at
    // the same edge and keeps room past it.
    const channelLeft = install.getBoundingClientRect().right - box.left + CLEAR
    const channel = (index: number): number => channelLeft + index * GAP
    const shotTop = shot.top - box.top
    const fits = innerWidth >= 1024 && channel(0) - 40 > startX
    svg.style.display = fits ? '' : 'none'
    const room = channel(0) - startX
    for (const [index, { tag }] of tags.entries()) {
      tag.style.left = `${startX + room * (0.22 + index * 0.27)}px`
      tag.style.top = `${startY}px`
      tag.hidden = !fits || room < 400
    }
    if (!fits) {
      return false
    }
    for (const [index, { packet, plug, route }] of lanes.entries()) {
      const card = cards[column(index)]
      const target = shot.left - box.left + ((card[0] + card[2] / 2) / 100) * shot.width + (index % 2 ? 6 : -6)
      const x = channel(index)
      const y = shotTop - 15 - (laneCount - 1 - index) * GAP
      const d = [
        `M${startX} ${startY}`,
        `H${x - RADIUS}`,
        `Q${x} ${startY} ${x} ${startY + RADIUS}`,
        `V${y - RADIUS}`,
        `Q${x} ${y} ${x - RADIUS} ${y}`,
        `H${target + RADIUS}`,
        `Q${target} ${y} ${target} ${y + RADIUS}`,
        `V${shotTop - 4}`
      ].join(' ')
      route.setAttribute('d', d)
      packet.setAttribute('d', d)
      plug.setAttribute('cx', String(target))
      plug.setAttribute('cy', String(shotTop - 4))
    }
    return true
  }

  /** Everything in place at once: for a visitor who asked for less motion, or one the entrance did not wait for. */
  const settle = (): void => {
    fleet?.layer.setAttribute('data-settled', '')
    svg.setAttribute('data-settled', '')
    svg.setAttribute('data-drawn', '')
    for (const { tag } of tags) {
      tag.setAttribute('data-on', '')
    }
    for (const { plug } of lanes) {
      plug.setAttribute('data-on', '')
    }
    for (const card of tiles) {
      card.setAttribute('data-in', '')
    }
  }

  const enter = async (elapsed: number): Promise<void> => {
    const routed = layout()
    await sleep(Math.max(0, DEPART - elapsed))
    if (!routed) {
      // No room for the routes: the cards are dealt on their own.
      for (const [index, card] of tiles.entries()) {
        setTimeout(() => card.setAttribute('data-in', ''), 250 + index * 85)
      }
      await sleep(250 + tiles.length * 85 + 700)
      settle()
      svg.setAttribute('data-live', '')
      return
    }
    svg.setAttribute('data-drawn', '')
    for (const [index, { tag }] of tags.entries()) {
      setTimeout(() => tag.setAttribute('data-on', ''), 170 + index * 110)
    }
    for (const [index, { plug }] of lanes.entries()) {
      // A route's card lands as the route arrives, and the cards under it in the column follow.
      setTimeout(
        () => {
          plug.setAttribute('data-on', '')
          tiles[cardOf(index)]?.setAttribute('data-in', '')
          if (index % 2) {
            for (let below = cardOf(index) + columns; below < tiles.length; below += columns) {
              setTimeout(() => tiles[below].setAttribute('data-in', ''), 170)
            }
          }
        },
        DRAW + index * DRAW_STEP - 140
      )
    }
    await sleep(DRAW)
    svg.setAttribute('data-live', '')
  }

  svg.addEventListener('animationiteration', (event) => {
    const lane = Number((event.target as SVGElement).dataset.lane)
    const arrived = lanes[lane]
    if (arrived) {
      // The ring fades out rather than changing colour: its colour is mixed in oklab, and Chromium interpolates
      // a stroke between that and an sRGB colour into nonsense.
      arrived.plug.animate([{ fill: 'var(--accent)', strokeOpacity: 0, transform: 'scale(1.7)' }, {}], {
        duration: 620,
        easing: EASE_OUT
      })
      const card = tiles[cardOf(lane)]
      if (card) {
        flare(card, 900, '::after')
      }
    }
  })

  // Every so often the third carrier's evening peak shows up in its latency.
  let ticks = 0
  setInterval(() => {
    if (!svg.hasAttribute('data-live') || svg.hasAttribute('data-paused')) {
      return
    }
    ticks++
    const peak = ticks % 12 >= 9
    for (const [index, { tag, value }] of tags.entries()) {
      const slow = peak && index === 2
      const base = LATENCY[index % LATENCY.length]
      if (value) {
        value.textContent = String(Math.round(slow ? rand(172, 196) : base + rand(-3, 3)))
      }
      tag.toggleAttribute('data-warn', slow)
    }
  }, 1100)

  new ResizeObserver(layout).observe(hero)
  document.fonts.ready.then(layout)

  if (reduced) {
    layout()
    settle()
    return
  }
  watch(hero, (visible) => svg.toggleAttribute('data-paused', !visible), 0.05)
  // The entrance is CSS and began before this script: the routes take their time from it. Whether there is an
  // entrance at all is known by the first frame, where an arrival through a view transition calls it off.
  requestAnimationFrame(() => {
    const fade = frame.getAnimations().find((animation) => (animation as CSSAnimation).animationName === 'fade-in')
    const elapsed = root.classList.contains('enter') && fade ? Number(fade.currentTime ?? 0) : Number.POSITIVE_INFINITY
    // The frame is hidden until its fade begins. A script that starts later must not empty it again.
    const hiddenFor = Number(fade?.effect?.getComputedTiming().delay ?? 0)
    if (elapsed < hiddenFor) {
      enter(elapsed)
    } else {
      layout()
      settle()
      svg.setAttribute('data-live', '')
    }
  })
}
