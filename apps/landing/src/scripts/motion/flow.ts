/*
 * Architecture and its numbers, on one clock. Once per report interval a report travels the diagram, agents to
 * server to clients, and now and then an alert leaves through a notification channel. The figures under the
 * stats keep the same time: the interval's bar fills once per report.
 */

import { EASE_OUT, el, rand, sleep, watch } from './stage'

/** Every this many reports, one raises an alert. */
const ALERT_EVERY = 4
/** How often the memory line takes a sample, in milliseconds. */
const SAMPLE = 520
/** The memory line's height and the band its samples stay in, in the units of its drawing. */
const LEVEL = 10
const NOISE = 1.4

const WIRE_EASING = 'cubic-bezier(0.45, 0, 0.3, 1)'
const narrow = matchMedia('(max-width: 900px)')

/** Calls `step` now, then again every `interval` for as long as `active` holds. Starting it twice does nothing. */
function ticker(interval: number, active: () => boolean, step: () => void): () => void {
  let running = false
  return async () => {
    if (running) {
      return
    }
    running = true
    while (active()) {
      step()
      await sleep(interval)
    }
    running = false
  }
}

/** The diagram: returns what one report does to it. */
function diagram(flow: HTMLElement): () => void {
  const boxes = [...flow.querySelectorAll<HTMLElement>('.flow-box')]
  const rows = boxes.map((box) => [...box.querySelectorAll<HTMLElement>('li')])
  const [agents = [], server = [], clients = []] = rows
  const tracks = [...flow.querySelectorAll('.flow-wire')].map((wire) => {
    const track = el('span', 'wire-dots')
    track.setAttribute('aria-hidden', 'true')
    track.append(el('i'), el('i'), el('i'))
    wire.append(track)
    return track
  })
  const alert = flow.querySelector('.flow-alert')
  const ticks = new Map(
    clients.map((row) => {
      const tick = el('i', 'row-tick')
      tick.setAttribute('aria-hidden', 'true')
      row.append(tick)
      return [row, tick]
    })
  )

  const send = (track: HTMLElement | undefined, warn: boolean): void => {
    if (!track) {
      return
    }
    track.toggleAttribute('data-alert', warn)
    const length = (narrow.matches ? track.clientHeight : track.clientWidth) - 6
    const to = narrow.matches ? `translateY(${length}px)` : `translateX(${length}px)`
    for (const [index, dot] of [...track.children].entries()) {
      dot.animate(
        [
          { opacity: 0, transform: 'none' },
          { opacity: 1, offset: 0.14 },
          { opacity: 1, offset: 0.84 },
          { opacity: 0, transform: to }
        ],
        { duration: 780, delay: index * 95, easing: WIRE_EASING }
      )
    }
  }
  const flash = (row: HTMLElement | undefined, warn = false): void => {
    if (!row) {
      return
    }
    const tone = warn ? 'var(--warn)' : 'var(--accent)'
    const ink = warn ? 'var(--warn)' : 'var(--accent-text)'
    const tick = ticks.get(row)
    if (tick) {
      const timing = { duration: 1100, easing: EASE_OUT }
      tick.animate(
        [
          { opacity: 1, transform: 'scaleY(0.35)', backgroundColor: tone },
          { opacity: 1, transform: 'none', backgroundColor: tone, offset: 0.3 },
          { opacity: 0, transform: 'none', backgroundColor: tone }
        ],
        timing
      )
      row.querySelector('b')?.animate([{ color: ink }, {}], timing)
      return
    }
    row.animate(
      [
        {
          backgroundColor: warn ? 'var(--warn-soft)' : 'var(--accent-soft)',
          boxShadow: `inset 0 0 0 1px ${tone}`,
          color: ink
        },
        { boxShadow: 'inset 0 0 0 1px transparent' }
      ],
      { duration: 950, easing: EASE_OUT }
    )
  }

  let reports = 0
  return () => {
    reports++
    const warn = reports % ALERT_EVERY === 0
    for (const [index, row] of agents.entries()) {
      setTimeout(() => flash(row), index * 80)
    }
    setTimeout(() => send(tracks[0], false), 140)
    setTimeout(() => flash(server[0]), 1000)
    setTimeout(() => flash(server[2]), 1130)
    setTimeout(() => send(tracks[1], warn), 1150)
    setTimeout(() => flash(clients[0]), 2020)
    setTimeout(() => flash(clients[1], warn), 2100)
    if (warn) {
      setTimeout(() => {
        flash(clients[2], true)
        alert?.animate(
          [
            { opacity: 0, transform: 'translateY(6px) scale(0.96)' },
            { opacity: 1, transform: 'none', offset: 0.14 },
            { opacity: 1, transform: 'none', offset: 0.86 },
            { opacity: 0, transform: 'translateY(-4px)' }
          ],
          { duration: 2300, easing: 'ease-out' }
        )
      }, 2180)
    }
  }
}

/** The memory line: returns what one sample does to it. */
function memoryLine(figure: Element | null): () => void {
  const line = figure?.querySelector('path')
  const head = figure?.querySelector('circle')
  const box = figure?.querySelector('svg')?.viewBox.baseVal
  if (!(line && head && box)) {
    return () => undefined
  }
  // As many samples as the line was drawn with.
  const points = (line.getAttribute('d') ?? '').split('L').length
  const step = box.width / (points - 1)
  const sample = (): number => LEVEL + rand(-NOISE, NOISE)
  // One sample more than fits, waiting off the left edge, so the line can slide.
  const samples = Array.from({ length: points + 1 }, sample)
  const draw = (): void => {
    line.setAttribute(
      'd',
      samples.map((y, index) => `${index ? 'L' : 'M'}${(index - 1) * step} ${y.toFixed(2)}`).join(' ')
    )
    head.setAttribute('cy', (samples.at(-1) ?? LEVEL).toFixed(2))
  }
  draw()
  return () => {
    const last = samples.at(-1) ?? LEVEL
    samples.shift()
    samples.push(sample())
    draw()
    const timing = { duration: SAMPLE, easing: 'linear' }
    line.animate([{ transform: `translateX(${step}px)` }, { transform: 'none' }], timing)
    head.animate([{ transform: `translateY(${last - (samples.at(-1) ?? LEVEL)}px)` }, { transform: 'none' }], timing)
  }
}

export function mountFlow(): void {
  const flow = document.querySelector<HTMLElement>('.flow')
  const stats = document.querySelector<HTMLElement>('.stats')
  if (!(flow && stats)) {
    return
  }
  const period = Number(flow.dataset.period) || 3000
  const report = diagram(flow)
  const figures = [...stats.querySelectorAll<HTMLElement>('.stat-fig')]
  const bar = stats.querySelector('.fig-bar i')
  const dots = [...stats.querySelectorAll('.fig-dots i')]
  // The figures start empty and fill in when the stats first show.
  for (const figure of figures) {
    figure.setAttribute('data-empty', '')
  }

  const pulse = ticker(
    period,
    () => flowVisible() || statsVisible(),
    () => {
      if (flowVisible()) {
        report()
      }
      if (!statsVisible()) {
        return
      }
      bar?.animate([{ transform: 'scaleX(0)' }, { transform: 'scaleX(1)' }], { duration: period, easing: 'linear' })
      // A test run: a handful of dots go grey and come back green.
      const from = Math.floor(rand(0, Math.max(1, dots.length - 6)))
      for (const [index, dot] of dots.slice(from, from + 6).entries()) {
        dot.animate([{ backgroundColor: 'var(--line-strong)' }, {}], {
          duration: 700,
          delay: index * 70,
          easing: 'ease-out',
          fill: 'backwards'
        })
      }
    }
  )
  const sampling = ticker(SAMPLE, () => statsVisible(), memoryLine(stats.querySelector('.fig-spark')))

  const flowVisible = watch(flow, (visible) => visible && pulse(), 0.4)
  const statsVisible = watch(
    stats,
    (visible) => {
      if (visible) {
        for (const figure of figures) {
          figure.removeAttribute('data-empty')
        }
        pulse()
        sampling()
      }
    },
    0.6
  )
}
