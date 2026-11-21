/* Network: the three carriers are probed one after another, then the latency chart plots itself. */

import { el, flare, onArrival, overlay, place, rand, rectOf, regionsOf, sleep, spread } from './stage'

/** How long the chart takes to plot, and how it eases. */
const PLOT = 1900
const PLOT_EASING = 'cubic-bezier(0.45, 0, 0.3, 1)'

export function mountNetwork(): void {
  const figure = document.querySelector<HTMLElement>('#network .shot')
  if (!figure) {
    return
  }
  const regions = regionsOf(figure)
  const plot = regions.plot?.[0]
  if (!plot) {
    return
  }
  const area = rectOf(plot)
  const scene = overlay(figure, 'network-layer', spread(area))
  if (!scene) {
    return
  }
  const { layer } = scene
  // The cover hides the chart's lines and is drawn back from the left; the pen rides its edge.
  const cover = el('i', 'cover plot-cover')
  place(cover, area)
  const pen = el('i', 'plot-pen')
  pen.style.left = `${area[0]}%`
  pen.style.top = `${area[1]}%`
  pen.style.height = `${area[3]}%`
  const probes = (regions.carriers ?? []).map((carrier) => {
    const probe = el('i', 'probe')
    place(probe, rectOf(carrier))
    return probe
  })
  layer.append(cover, pen, ...probes)

  const across = (): string => `translateX(${(layer.clientWidth * area[2]) / 100}px)`

  const visible = onArrival(figure, 0.45, async () => {
    await scene.ready
    await sleep(250)
    for (const probe of probes) {
      flare(probe, 900)
      await sleep(170)
    }
    await sleep(200)
    cover.setAttribute('data-drawn', '')
    pen.animate(
      [
        { opacity: 1, transform: 'none' },
        { opacity: 1, offset: 0.92 },
        { opacity: 0, transform: across() }
      ],
      { duration: PLOT, easing: PLOT_EASING }
    )
    await sleep(PLOT + 400)
    // Afterwards a reading line passes over the chart now and then, as a live panel's cursor would.
    while (figure.isConnected) {
      await sleep(rand(4200, 6200))
      if (visible()) {
        pen.animate(
          [
            { opacity: 0, transform: 'none' },
            { opacity: 0.55, offset: 0.12 },
            { opacity: 0.55, offset: 0.88 },
            { opacity: 0, transform: across() }
          ],
          { duration: 2600, easing: 'cubic-bezier(0.5, 0, 0.5, 1)' }
        )
      }
    }
  })
}
