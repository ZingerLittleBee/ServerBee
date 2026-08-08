import '@testing-library/jest-dom'
import i18next from 'i18next'

// The i18n module syncs <html lang> at import time, which needs a DOM.
// Node-environment test files (the vite config test) skip that browser-only
// initialization entirely.
if (typeof document !== 'undefined') {
  await import('@/lib/i18n')

  // Force English in tests so assertions on translated strings are
  // deterministic regardless of the jsdom navigator language.
  if (i18next.language !== 'en') {
    i18next.changeLanguage('en')
  }
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

// Guarded on Element itself: node-environment test files (e.g. the vite
// config test) run this setup without DOM globals.
if (typeof Element !== 'undefined' && typeof Element.prototype.scrollIntoView === 'undefined') {
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

// Node's experimental global localStorage accessor can shadow jsdom's in a
// worker and is undefined without --localstorage-file (merely reading it
// emits an ExperimentalWarning). Install a deterministic in-memory Storage
// for every jsdom test file instead: unconditionally via defineProperty,
// never read-first, so it holds regardless of Node's accessor state.
// Node-environment test files (no window) are left untouched.
if (typeof window !== 'undefined') {
  const store = new Map<string, string>()
  const storage: Storage = {
    get length() {
      return store.size
    },
    clear: () => store.clear(),
    getItem: (key: string) => store.get(key) ?? null,
    key: (index: number) => Array.from(store.keys())[index] ?? null,
    removeItem: (key: string) => {
      store.delete(key)
    },
    setItem: (key: string, value: string) => {
      store.set(key, String(value))
    }
  }
  Object.defineProperty(globalThis, 'localStorage', {
    value: storage,
    writable: true,
    enumerable: true,
    configurable: true
  })
}
