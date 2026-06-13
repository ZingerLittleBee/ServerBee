import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { bootstrapLoader } from '@/widgets-runtime/loader'
import { useWidgetModuleBootstrap } from './use-widget-module-bootstrap'

vi.mock('@/widgets-runtime/loader', () => ({
  bootstrapLoader: vi.fn(() => Promise.resolve())
}))

afterEach(() => {
  vi.clearAllMocks()
})

describe('useWidgetModuleBootstrap', () => {
  it('bootstraps once when enabled after initially disabled', async () => {
    const { rerender } = renderHook(({ enabled }) => useWidgetModuleBootstrap(enabled), {
      initialProps: { enabled: false }
    })

    expect(bootstrapLoader).not.toHaveBeenCalled()

    rerender({ enabled: true })

    await waitFor(() => {
      expect(bootstrapLoader).toHaveBeenCalledTimes(1)
    })

    rerender({ enabled: true })

    expect(bootstrapLoader).toHaveBeenCalledTimes(1)
  })
})
