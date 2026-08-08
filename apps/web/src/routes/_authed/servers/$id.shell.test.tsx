import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  // Mirror the real Route class, which exposes the createFileRoute options
  // on its public `options` property (Route.options.component).
  createFileRoute: () => (config: unknown) => ({ options: config })
}))

// Keep the lazy page chunk pending forever so what renders for the whole
// test is the Suspense fallback.
vi.mock('./$id-page', async () => new Promise(() => undefined))

function isRouteComponent(value: unknown): value is React.ComponentType {
  return typeof value === 'function'
}

/**
 * Typed seam for the route's public `options.component` (typed `unknown` by
 * TanStack): narrows it to a renderable component, or fails with a clear
 * error when the createFileRoute mock did not provide one.
 */
function requireRouteComponent(route: { options: { component?: unknown } }): React.ComponentType {
  const { component } = route.options
  if (!isRouteComponent(component)) {
    throw new Error('Route options.component is missing or not a function — check the createFileRoute mock')
  }
  return component
}

const { Route } = await import('./$id')
const Shell = requireRouteComponent(Route)

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
