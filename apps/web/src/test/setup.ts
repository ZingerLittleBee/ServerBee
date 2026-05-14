import '@testing-library/jest-dom'

class ResizeObserverMock {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  // @ts-expect-error jsdom does not implement ResizeObserver
  globalThis.ResizeObserver = ResizeObserverMock
}
