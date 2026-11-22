/*
 * The kit's sketches. Each starts empty and draws itself once, when it comes into view. The two that keep moving,
 * the terminal's cursor and the tailnet's packets, do so only while they are on screen.
 */

import { BELOW_LINE } from './reveal'
import { watch } from './stage'

export function mountKit(): void {
  // A sketch draws once a little of it is past the line its tile is revealed at, so never inside a hidden tile.
  // On a phone that also holds sideways: the tile peeking in at the edge of a strip is drawn, and the ones further
  // along wait until they are swiped in.
  const arrivals = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          arrivals.unobserve(entry.target)
          entry.target.removeAttribute('data-empty')
        }
      }
    },
    { rootMargin: BELOW_LINE, threshold: 0.15 }
  )
  for (const sketch of document.querySelectorAll('.sketch')) {
    sketch.setAttribute('data-empty', '')
    arrivals.observe(sketch)
    watch(sketch, (visible) => sketch.toggleAttribute('data-visible', visible), 0)
  }
}
