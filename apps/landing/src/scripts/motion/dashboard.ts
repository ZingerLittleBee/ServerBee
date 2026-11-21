/* Dashboards: the wall is dealt out widget by widget, then widgets of the same size are dragged to trade places. */

import {
  EASE_IN_OUT,
  EASE_OUT,
  el,
  grow,
  onArrival,
  overlay,
  place,
  type Rect,
  rectOf,
  regionsOf,
  sleep,
  tile
} from './stage'

/** A drag picks its widget up a little larger. */
const HELD = 'scale(1.04)'

const CURSOR =
  '<svg viewBox="0 0 24 24"><path d="M5 3l14 8-6.2 1.6L9.6 19 5 3z" fill="#fff" stroke="#111" stroke-width="1.6" stroke-linejoin="round"/></svg>'

/** Pairs of slots that hold widgets of one size, which is what the editor lets trade places: the first two kinds. */
function swaps(slots: Rect[]): [number, number][] {
  const sizes = new Map<string, number[]>()
  for (const [index, [, , width, height]] of slots.entries()) {
    const size = `${width.toFixed(1)}x${height.toFixed(1)}`
    sizes.set(size, [...(sizes.get(size) ?? []), index])
  }
  return [...sizes.values()]
    .filter((alike) => alike.length > 1)
    .slice(0, 2)
    .map((alike) => {
      const at = Math.floor((alike.length - 2) / 2)
      return [alike[at], alike[at + 1]]
    })
}

export function mountDashboard(): void {
  const figure = document.querySelector<HTMLElement>('#dash .shot')
  if (!figure) {
    return
  }
  const widgets = (regionsOf(figure).widgets ?? []).map(rectOf)
  if (widgets.length < 2) {
    return
  }
  // The gap between the first two widgets: the wall behind them.
  const [one, two] = widgets
  const scene = overlay(figure, 'dash-layer', [[(one[0] + one[2] + two[0]) / 2, one[1] + one[3] / 2]])
  if (!scene) {
    return
  }
  const { layer } = scene
  const slots = widgets.map((widget) => grow(widget, 0.12, scene.aspect))
  const tiles = slots.map((slot) => tile(layer, slot))
  // Which tile sits in which slot; a drag swaps two entries.
  const seat = tiles.map((_, index) => index)
  const ghost = el('i', 'slot')
  const cursor = el('i', 'cursor')
  cursor.innerHTML = CURSOR
  layer.append(ghost, cursor)

  const centre = ([x, y, width, height]: Rect): [number, number] => [
    ((x + width / 2) / 100) * layer.clientWidth,
    ((y + height / 2) / 100) * layer.clientHeight
  ]
  const at = (x: number, y: number): string => `translate(${x}px, ${y}px)`

  const drag = async (a: number, b: number): Promise<void> => {
    const moving = tiles[seat[a]]
    const yielding = tiles[seat[b]]
    const [ax, ay] = centre(slots[a])
    const [bx, by] = centre(slots[b])
    const dx = bx - ax
    const dy = by - ay
    cursor.animate(
      [
        { opacity: 0, transform: at(ax + 46, ay + 58) },
        { opacity: 1, transform: at(ax, ay) }
      ],
      { duration: 520, easing: EASE_OUT, fill: 'forwards' }
    )
    await sleep(600)
    moving.setAttribute('data-held', '')
    place(ghost, slots[b])
    ghost.setAttribute('data-on', '')
    moving.animate([{ transform: 'none' }, { transform: HELD }], { duration: 160, easing: EASE_OUT, fill: 'forwards' })
    await sleep(240)
    const travel: KeyframeAnimationOptions = { duration: 820, easing: EASE_IN_OUT, fill: 'forwards' }
    moving.animate([{ transform: HELD }, { transform: `${at(dx, dy)} ${HELD}` }], travel)
    cursor.animate([{ transform: at(ax, ay) }, { transform: at(bx, by) }], travel)
    // The widget in the way gives up its place before the dragged one lands.
    setTimeout(() => {
      yielding.animate([{ transform: 'none' }, { transform: at(-dx, -dy) }], {
        duration: 480,
        easing: EASE_OUT,
        fill: 'forwards'
      })
    }, 300)
    await sleep(880)
    moving.animate([{ transform: `${at(dx, dy)} ${HELD}` }, { transform: at(dx, dy) }], {
      duration: 200,
      easing: EASE_OUT,
      fill: 'forwards'
    })
    ghost.removeAttribute('data-on')
    await sleep(220)
    // Commit: each tile now belongs to the other slot.
    for (const [widget, slot] of [
      [moving, slots[b]],
      [yielding, slots[a]]
    ] as const) {
      for (const animation of widget.getAnimations()) {
        animation.cancel()
      }
      widget.style.left = `${slot[0]}%`
      widget.style.top = `${slot[1]}%`
    }
    moving.removeAttribute('data-held')
    ;[seat[a], seat[b]] = [seat[b], seat[a]]
    cursor.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 300, fill: 'forwards' })
  }

  const moves = swaps(slots)
  const visible = onArrival(figure, 0.35, async () => {
    await scene.ready
    await sleep(250)
    for (const [index, widget] of tiles.entries()) {
      setTimeout(() => widget.setAttribute('data-in', ''), index * 70)
    }
    await sleep(tiles.length * 70 + 1100)
    for (let step = 0; moves.length > 0 && figure.isConnected; step++) {
      if (visible()) {
        await drag(...moves[step % moves.length])
      }
      await sleep(2600)
    }
  })
}
