import '@testing-library/jest-dom'
import i18next from 'i18next'
import '@/lib/i18n'

// Force English in tests so assertions on translated strings are deterministic
// regardless of the jsdom navigator language.
if (i18next.language !== 'en') {
  i18next.changeLanguage('en')
}

class ResizeObserverMock {
  // biome-ignore lint/complexity/noUselessConstructor: matches ResizeObserver API signature
  constructor(_callback?: ResizeObserverCallback) {
    // no-op: mock does not invoke the callback
  }
  observe(): void {
    // no-op
  }
  unobserve(): void {
    // no-op
  }
  disconnect(): void {
    // no-op
  }
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = ResizeObserverMock
}

if (typeof Element.prototype.scrollIntoView === 'undefined') {
  Element.prototype.scrollIntoView = () => undefined
}

// jsdom has no matchMedia. boneyard's Skeleton (dark-mode detection) and the
// useReducedMotion hook both call it, so provide a no-preference default;
// individual tests override window.matchMedia to simulate specific queries.
if (typeof window !== 'undefined' && typeof window.matchMedia !== 'function') {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false
  })) as unknown as typeof window.matchMedia
}
