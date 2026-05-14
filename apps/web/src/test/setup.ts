import '@testing-library/jest-dom'

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
