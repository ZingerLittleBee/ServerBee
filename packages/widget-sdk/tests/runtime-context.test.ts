import { beforeEach, describe, expect, it } from 'vitest'
import { createWidgetRuntime, getRuntime, resetRuntime } from '../src/runtime-context'

describe('runtime-context', () => {
  beforeEach(() => resetRuntime())

  it('throws if hooks called before host installs runtime', () => {
    expect(() => getRuntime()).toThrow(/runtime not installed/)
  })

  it('returns installed runtime', () => {
    const runtime = createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => [],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {}
    })
    expect(getRuntime()).toBe(runtime)
  })
})
