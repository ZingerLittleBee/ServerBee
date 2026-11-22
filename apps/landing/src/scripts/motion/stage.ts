/* What the scenes share: timing, visibility, and layers that sit exactly on a capture. */

export const root = document.documentElement

/** The visitor asked for less motion: scenes show their settled state and nothing moves on its own. */
export const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches

export const zh = root.lang.toLowerCase().startsWith('zh')

export const EASE_OUT = 'cubic-bezier(0.23, 1, 0.32, 1)'
export const EASE_IN_OUT = 'cubic-bezier(0.77, 0, 0.175, 1)'

const SVG = 'http://www.w3.org/2000/svg'

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

export function rand(from: number, to: number): number {
  return from + Math.random() * (to - from)
}

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag)
  if (className) {
    node.className = className
  }
  return node
}

export function svgEl<K extends keyof SVGElementTagNameMap>(tag: K, className?: string): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG, tag)
  if (className) {
    node.setAttribute('class', className)
  }
  return node
}

/** Reports whether the target is on screen past the threshold, and calls back whenever that changes. */
export function watch(target: Element, onChange?: (visible: boolean) => void, threshold = 0.3): () => boolean {
  let visible = false
  new IntersectionObserver(
    ([entry]) => {
      if (entry.isIntersecting !== visible) {
        visible = entry.isIntersecting
        onChange?.(visible)
      }
    },
    { threshold }
  ).observe(target)
  return () => visible
}

/** Runs `play` the first time the target is on screen past the threshold. Reports whether it is on screen now. */
export function onArrival(target: Element, threshold: number, play: () => void): () => boolean {
  let played = false
  return watch(
    target,
    (visible) => {
      if (visible && !played) {
        played = true
        play()
      }
    },
    threshold
  )
}

/** A box on a capture, in percentages of it. */
export type Rect = [x: number, y: number, width: number, height: number]

/** A box scripts/capture.ts recorded from the product's own DOM, with what it found there when that matters. */
export type Region = [x: number, y: number, width: number, height: number, tag?: string]

export function regionsOf(figure: HTMLElement): Record<string, Region[]> {
  return JSON.parse(figure.dataset.regions ?? '{}')
}

export function rectOf([x, y, width, height]: Region): Rect {
  return [x, y, width, height]
}

/** Grows a box by `pad` percent of the capture's width on every side. */
export function grow([x, y, width, height]: Rect, pad: number, aspect: number): Rect {
  return [x - pad, y - pad * aspect, width + 2 * pad, height + 2 * pad * aspect]
}

export function place(node: HTMLElement, [x, y, width, height]: Rect): void {
  node.style.left = `${x}%`
  node.style.top = `${y}%`
  node.style.width = `${width}%`
  node.style.height = `${height}%`
}

const pixel = document.createElement('canvas')
pixel.width = 1
pixel.height = 1
const context = pixel.getContext('2d', { willReadFrequently: true })

/** The capture's own color at a point, so that what covers it matches in either theme. */
function colorAt(img: HTMLImageElement, [x, y]: [number, number]): string | undefined {
  if (!context) {
    return
  }
  try {
    context.clearRect(0, 0, 1, 1)
    context.drawImage(img, (img.naturalWidth * x) / 100, (img.naturalHeight * y) / 100, 1, 1, 0, 0, 1, 1)
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data
    return `rgb(${r} ${g} ${b})`
  } catch {
    // The capture is served from another origin: the cover keeps the surface color.
  }
}

/** The color most of the points share: the ground of a region, whatever is drawn across it. */
function ground(img: HTMLImageElement, points: [number, number][]): string | undefined {
  const counts = new Map<string, number>()
  let best: string | undefined
  for (const point of points) {
    const color = colorAt(img, point)
    if (color) {
      const count = (counts.get(color) ?? 0) + 1
      counts.set(color, count)
      if (count > (counts.get(best ?? '') ?? 0)) {
        best = color
      }
    }
  }
  return best
}

/** Nine points spread across a box, clear of its edges. */
export function spread([x, y, width, height]: Rect): [number, number][] {
  const points: [number, number][] = []
  for (const row of [0.2, 0.5, 0.8]) {
    for (const column of [0.2, 0.5, 0.8]) {
      points.push([x + width * column, y + height * row])
    }
  }
  return points
}

export interface Overlay {
  /** The capture's width over its height. */
  aspect: number
  layer: HTMLDivElement
  /** Settles once the capture has loaded and the layer knows its picture and its ground color. */
  ready: Promise<void>
}

/**
 * A layer that sits exactly on a capture. Covers in it take the color the capture has at `groundAt`, and tiles
 * show the capture itself. While the figure is on screen the layer carries data-visible.
 */
export function overlay(figure: HTMLElement, className: string, groundAt: [number, number][]): Overlay | undefined {
  const img = figure.querySelector('img')
  if (!img) {
    return
  }
  const layer = el('div', `overlay ${className}`)
  layer.setAttribute('aria-hidden', 'true')
  // The frame, not the figure: on a narrow screen the figure is the strip the frame pans in.
  const frame = figure.querySelector('.shot-frame') ?? figure
  frame.append(layer)
  watch(figure, (visible) => layer.toggleAttribute('data-visible', visible), 0)

  let settle: () => void
  const ready = new Promise<void>((resolve) => {
    settle = resolve
  })
  const sync = async (): Promise<void> => {
    if (!(img.complete && img.naturalWidth)) {
      return
    }
    // Decoded off the main thread first, so reading a pixel does not stall the page.
    await img.decode().catch(() => undefined)
    layer.style.setProperty('--capture', `url("${img.currentSrc}")`)
    layer.style.setProperty('--capture-ratio', `${img.naturalWidth} / ${img.naturalHeight}`)
    const color = ground(img, groundAt)
    if (color) {
      layer.style.setProperty('--cover', color)
    }
    settle()
  }
  // The capture loads again when the theme changes.
  img.addEventListener('load', sync)
  sync()
  return { aspect: img.width / img.height, layer, ready }
}

/** A movable copy of one region of the capture, over a cover that hides the original. */
export function tile(layer: HTMLElement, rect: Rect): HTMLElement {
  const [x, y, width, height] = rect
  const cover = el('i', 'cover')
  place(cover, rect)
  const copy = el('i', 'tile')
  place(copy, rect)
  copy.style.backgroundSize = `${10_000 / width}% ${10_000 / height}%`
  copy.style.backgroundPosition = `${(100 * x) / (100 - width)}% ${(100 * y) / (100 - height)}%`
  layer.append(cover, copy)
  return copy
}

/** A ring that flares once on a pseudo-element or an element drawn with a box-shadow outline. */
export function flare(node: Element, duration: number, pseudoElement?: string): void {
  node.animate([{ opacity: 0.9 }, { opacity: 0 }], { duration, easing: EASE_OUT, pseudoElement })
}
