import type { AnyRouter, ParsedLocation } from '@tanstack/react-router'

// What the reader does to move on from a position the router set.
const readerInput = ['keydown', 'pointerdown', 'touchstart', 'wheel']

/**
 * The router's `scrollRestoration` option. The router calls it before it restores or resets the scroll position
 * after a render.
 *
 * The first call comes from hydration, and the page may already be scrolled by then: by a reader who did not wait, or
 * by the browser to a scroll-to-text match. The router has nothing saved for a new history entry, so it would reset
 * that to the top. Keep it instead, and hand it to the router's scroll listener, which saves it for Back. A URL with a
 * #fragment is still left to the router: it lands on the fragment again once the page has its final layout, and a
 * reloaded entry gets its saved position.
 *
 * Later calls hold the position the router sets, unless it scrolls to a #fragment. WebKit lays out a code block or a
 * wide table without its horizontal scrollbar at first and makes room for the scrollbar on the next layout. The router
 * restores a position after Back or Forward on the first layout, and WebKit's scroll anchoring would then keep what is
 * in view at that moment in place as the page grows above it, landing the page lower than the reader left it: 15px for
 * each code block above on the quick start. The saved position is the right one for the page's final layout, so scroll
 * anchoring stays off until the reader moves on.
 */
export function createScrollRestoration(): (options: { location: ParsedLocation }) => boolean {
  if (typeof window === 'undefined') {
    // The server calls it too, to decide whether to render the router's script that restores a saved position as the
    // page loads.
    return () => true
  }
  let hydrating = true
  let release: (() => void) | undefined
  return ({ location }) => {
    release?.()
    if (!hydrating) {
      if (location.hash === '' || location.state.__hashScrollIntoViewOptions === false) {
        release = holdScrollPosition()
      }
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

/** Turns scroll anchoring off until the reader's next input, and returns the function that turns it back on. */
function holdScrollPosition(): () => void {
  const root = document.documentElement
  root.style.overflowAnchor = 'none'
  const release = () => {
    root.style.overflowAnchor = ''
    for (const type of readerInput) {
      removeEventListener(type, release, true)
    }
  }
  for (const type of readerInput) {
    addEventListener(type, release, { capture: true, passive: true })
  }
  return release
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
