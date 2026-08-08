import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: unknown) => config
}))

// Keep the lazy page chunk pending forever so what renders for the whole
// test is the Suspense fallback.
vi.mock('./$id-page', async () => new Promise(() => undefined))

const { Route } = await import('./$id')
const Shell = (Route as { component: React.ComponentType }).component

describe('ServerDetailPageShell', () => {
  afterEach(() => {
    cleanup()
  })

  it('shows the generated server-detail skeleton as the Suspense fallback', () => {
    const { container } = render(<Shell />)

    const skeleton = container.querySelector('[data-boneyard="server-detail"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
  })
})
