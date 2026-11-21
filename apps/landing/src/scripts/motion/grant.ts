/*
 * Security: the grant command is typed out, takes effect in the capability table under it, counts down and lapses.
 */

import { EASE_OUT, el, grow, onArrival, overlay, place, rand, rectOf, regionsOf, sleep } from './stage'

/** The command grants `--for 30m`. */
const GRANT_SECONDS = 30 * 60
/** The table's own mark for a capability that holds no grant. */
const NO_GRANT = '—'

function clock(seconds: number): string {
  const minutes = String(Math.floor(seconds / 60)).padStart(2, '0')
  return `${minutes}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`
}

/** One element per character, so the command can be typed without its line moving. */
function keysOf(code: Element): HTMLElement[] {
  const walker = document.createTreeWalker(code, NodeFilter.SHOW_TEXT)
  const texts: Text[] = []
  while (walker.nextNode()) {
    texts.push(walker.currentNode as Text)
  }
  const keys: HTMLElement[] = []
  for (const text of texts) {
    const fragment = document.createDocumentFragment()
    for (const character of text.data) {
      const key = el('span', 'key')
      key.textContent = character
      fragment.append(key)
      keys.push(key)
    }
    text.replaceWith(fragment)
  }
  return keys
}

export function mountGrant(): void {
  const grant = document.querySelector<HTMLElement>('#security .grant')
  const code = grant?.querySelector('.grant-cmd code')
  const caption = grant?.querySelector('figcaption')
  if (!(grant && code && caption)) {
    return
  }
  const wording = grant.dataset
  const worded = (template: string | undefined, left: number): string => (template ?? '').replace('{left}', clock(left))

  const keys = keysOf(code)
  const args = [...code.querySelectorAll('b')]
  const notes = [...grant.querySelectorAll('.grant-notes div')]
  const caret = el('i', 'caret')
  caret.setAttribute('aria-hidden', 'true')

  const status = el('span', 'grant-status')
  status.setAttribute('aria-hidden', 'true')
  const statusText = el('span')
  status.append(el('i'), statusText)
  caption.append(status)
  const setStatus = (state: 'granted' | 'revoked', text: string): void => {
    if (status.dataset.state !== state) {
      status.dataset.state = state
      status.animate([{ opacity: 0, transform: 'translateY(6px)' }, {}], { duration: 420, easing: EASE_OUT })
    }
    statusText.textContent = text
  }

  // The capability table's cell for this grant, redrawn so that its clock can run.
  const figure = grant.nextElementSibling
  const table = figure instanceof HTMLElement && figure.matches('.shot') ? figure : undefined
  const mark = table && regionsOf(table).grant?.[0]
  // Sampled to the right of the badge: on its left the server's name can reach the cell.
  const scene =
    table && mark ? overlay(table, 'grant-layer', [[mark[0] + mark[2] + 1.5, mark[1] + mark[3] / 2]]) : undefined
  const cell = el('i', 'cover grant-badge')
  if (scene && mark) {
    place(cell, grow(rectOf(mark), 0.5, scene.aspect))
    scene.layer.append(cell)
  }
  const badge = el('b')
  const setCell = (text: string): void => {
    if (text === NO_GRANT) {
      cell.replaceChildren(text)
      return
    }
    badge.textContent = text
    if (!badge.isConnected) {
      cell.replaceChildren(badge)
    }
  }
  const ring = (): void => {
    cell.animate(
      [
        { opacity: 0.95, transform: 'scale(0.8)' },
        { opacity: 0, transform: 'scale(1.22)' }
      ],
      { duration: 700, easing: EASE_OUT, pseudoElement: '::after' }
    )
  }
  const show = (left: number): void => {
    setStatus('granted', worded(wording.granted, left))
    setCell(worded(wording.badge, left))
  }
  // Until the command runs, nothing is granted.
  setCell(NO_GRANT)

  onArrival(grant, 0.6, async () => {
    grant.setAttribute('data-armed', '')
    await sleep(450)
    grant.setAttribute('data-typing', '')
    for (const [index, key] of keys.entries()) {
      key.setAttribute('data-on', '')
      key.after(caret)
      // The end of an argument: its note below lights up.
      const arg = key.closest('b')
      if (arg && keys[index + 1]?.closest('b') !== arg) {
        arg.setAttribute('data-flash', '')
        notes[args.indexOf(arg)]?.setAttribute('data-on', '')
        setTimeout(() => arg.removeAttribute('data-flash'), 900)
        await sleep(230)
      }
      await sleep(key.textContent === ' ' ? 34 : rand(18, 40))
    }
    await sleep(520)
    grant.removeAttribute('data-typing')

    // Enter: the grant shows up in the capability table, with its clock running.
    let left = GRANT_SECONDS
    show(left)
    ring()
    for (let tick = 0; tick < 2; tick++) {
      await sleep(1000)
      left -= 1
      show(left)
    }
    // Thirty minutes later, in two seconds.
    const from = left
    const started = performance.now()
    await new Promise<void>((resolve) => {
      const frame = (now: number): void => {
        const t = Math.min(1, (now - started) / 2000)
        const eased = t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2
        show(Math.round(from * (1 - eased)))
        if (t < 1) {
          requestAnimationFrame(frame)
        } else {
          resolve()
        }
      }
      requestAnimationFrame(frame)
    })
    await sleep(350)
    setStatus('revoked', wording.revoked ?? '')
    setCell(NO_GRANT)
    ring()
    grant.removeAttribute('data-armed')
  })
}
