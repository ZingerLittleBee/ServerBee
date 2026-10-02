import type { AnyRouter } from '@tanstack/react-router'

/**
 * The router's `scrollRestoration` option. The router calls it before it restores or resets the scroll position
 * after a render.
 *
 * The first call comes from hydration, and the page may already be scrolled by then: by a reader who did not wait, or
 * by the browser to a scroll-to-text match. The router has nothing saved for a new history entry, so it would reset
 * that to the top. Keep it instead, and hand it to the router's scroll listener, which saves it for Back. A URL with a
 * #fragment is still left to the router: it lands on the fragment again once the page has its final layout, and a
 * reloaded entry gets its saved position.
 */
export function createScrollRestoration(): () => boolean {
  let hydrating = typeof window !== 'undefined'
  return () => {
    if (!hydrating) {
      return true
    }
    hydrating = false
    if ((window.scrollX === 0 && window.scrollY === 0) || window.location.hash !== '') {
      return true
    }
    document.dispatchEvent(new Event('scroll'))
    return false
  }
}

/**
 * Plain #fragment links (the docs' table of contents and heading anchors, the landing's nav and note marks) add
 * history entries without router state, so the router keys such an entry afresh on every visit and Back never finds
 * its saved position. On Back/Forward to any entry with a fragment, the router also scrolls to the fragment again
 * after restoring the position. Once an entry has rendered, store the router's key in it and mark its fragment as
 * handled, as the router itself does for a link with `hashScrollIntoView={false}`.
 */
export function keepFragmentEntries(router: AnyRouter): void {
  router.subscribe('onRendered', ({ toLocation }) => {
    const state = window.history.state
    if (state !== null && (toLocation.hash === '' || state.__hashScrollIntoViewOptions === false)) {
      return
    }
    // The browser's own replaceState: the router wraps history.replaceState and would treat the call as a navigation.
    History.prototype.replaceState.call(window.history, { ...toLocation.state, __hashScrollIntoViewOptions: false }, '')
    // The router saves a position only when the page scrolls, and the jump to the fragment may not have moved it.
    document.dispatchEvent(new Event('scroll'))
  })
}
