/* iOS: an alert lands on the first phone, and tapping it opens that server on the second. */

import { EASE_OUT, sleep, watch } from './stage'

const AWAY = 'translateY(-150%) scale(0.94)'

export function mountPush(): void {
  const phones = [...document.querySelectorAll<HTMLElement>('#ios .phone')]
  const banner = phones[0]?.querySelector<HTMLElement>('.push')
  const tap = banner?.querySelector('.push-tap')
  if (!(banner && tap) || phones.length < 2) {
    return
  }
  banner.hidden = false

  let running = false
  const visible = watch(
    phones[0],
    async (on) => {
      if (!on || running) {
        return
      }
      running = true
      await sleep(500)
      while (visible()) {
        banner.animate(
          [
            { opacity: 0, transform: AWAY },
            { opacity: 1, transform: 'none' }
          ],
          { duration: 640, easing: 'cubic-bezier(0.2, 1.3, 0.35, 1)', fill: 'forwards' }
        )
        await sleep(1900)
        tap.animate(
          [
            { opacity: 0.85, transform: 'scale(0.4)' },
            { opacity: 0, transform: 'scale(1.7)' }
          ],
          { duration: 460, easing: 'ease-out' }
        )
        banner.animate([{ transform: 'scale(0.97)' }, { transform: 'none' }], { duration: 260, easing: 'ease-out' })
        await sleep(240)
        // The server the alert is about comes forward.
        phones[1].animate(
          [{ transform: 'none' }, { transform: 'translateY(-10px) scale(1.025)', offset: 0.35 }, { transform: 'none' }],
          { duration: 720, easing: EASE_OUT }
        )
        await sleep(1500)
        banner.animate(
          [
            { opacity: 1, transform: 'none' },
            { opacity: 0, transform: AWAY }
          ],
          { duration: 380, easing: 'cubic-bezier(0.4, 0, 0.9, 0.6)', fill: 'forwards' }
        )
        await sleep(4200)
      }
      running = false
    },
    0.5
  )
}
