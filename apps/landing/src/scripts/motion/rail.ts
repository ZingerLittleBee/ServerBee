/* The page rail: a traceroute down the page. Each section is a hop, and the packet is where the reader is. */

/** A section counts as reached when its top passes this far down the screen. */
const REACHED = 0.42

export function mountRail(): void {
  const rail = document.querySelector<HTMLElement>('.rail')
  if (!rail) {
    return
  }
  const hops = [...rail.querySelectorAll<HTMLAnchorElement>('a[data-hop]')].flatMap((link) => {
    const section = document.getElementById(link.dataset.hop ?? '')
    return section ? [{ link, section }] : []
  })
  if (hops.length < 2) {
    return
  }

  let queued = false
  const update = (): void => {
    queued = false
    const anchors = hops.map((hop) => hop.section.offsetTop - innerHeight * REACHED)
    const y = scrollY
    let current = -1
    for (const [index, anchor] of anchors.entries()) {
      if (y >= anchor) {
        current = index
      }
    }
    let progress = 0
    if (current >= anchors.length - 1) {
      progress = 1
    } else if (current >= 0) {
      const span = anchors[current + 1] - anchors[current]
      progress = (current + (y - anchors[current]) / span) / (anchors.length - 1)
    }
    rail.style.setProperty('--progress', progress.toFixed(4))
    // The rail shows once the first hop is near.
    rail.toggleAttribute('data-live', y > anchors[0] - innerHeight * 0.35)
    for (const [index, { link }] of hops.entries()) {
      link.toggleAttribute('data-passed', index <= current)
      link.toggleAttribute('data-current', index === current)
    }
  }
  addEventListener(
    'scroll',
    () => {
      if (!queued) {
        queued = true
        requestAnimationFrame(update)
      }
    },
    { passive: true }
  )
  addEventListener('resize', update)
  update()
}
