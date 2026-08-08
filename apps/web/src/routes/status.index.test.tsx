import { cleanup, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const serversState = vi.hoisted(() => ({
  data: undefined as unknown,
  error: null as Error | null,
  isLoading: false
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: unknown) => config,
  Link: ({ children, to, ...rest }: { children: ReactNode; to: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  )
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: () => serversState
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@/lib/api-client', () => ({
  api: { get: vi.fn() }
}))

vi.mock('@/hooks/use-public-status', () => ({
  usePublicStatusConfig: () => ({
    data: { enabled: true, show_server_detail: true, default_layout: 'grid' }
  })
}))

vi.mock('@/components/status/server-summary-card', () => ({
  ServerSummaryCard: ({ server }: { server: { id: string } }) => <div data-testid="summary-card">{server.id}</div>
}))

vi.mock('@/components/status/server-summary-row', () => ({
  ServerSummaryRow: ({ server }: { server: { id: string } }) => (
    <tr data-testid="summary-row">
      <td>{server.id}</td>
    </tr>
  )
}))

vi.mock('@/components/status/layout-toggle', () => ({
  LayoutToggle: () => null
}))

async function renderPage() {
  const { Route } = await import('./status.index')
  const Page = (Route as { component: React.ComponentType }).component
  return render(<Page />)
}

describe('PublicStatusIndex', () => {
  beforeEach(() => {
    serversState.data = undefined
    serversState.error = null
    serversState.isLoading = false
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the generated status-overview skeleton while loading', async () => {
    serversState.isLoading = true
    const { container } = await renderPage()

    const skeleton = container.querySelector('[data-boneyard="status-overview"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
    expect(screen.queryByText('no_servers')).toBeNull()
  })

  it('renders server cards once loaded, without any skeleton container', async () => {
    serversState.data = [{ id: 'srv-1' }, { id: 'srv-2' }]
    const { container } = await renderPage()

    expect(container.querySelectorAll('[data-testid="summary-card"]')).toHaveLength(2)
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('keeps the error branch free of skeleton markup', async () => {
    serversState.error = new Error('boom')
    const { container } = await renderPage()

    expect(screen.getByText('load_failed')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('keeps the empty branch free of skeleton markup', async () => {
    serversState.data = []
    const { container } = await renderPage()

    expect(screen.getByText('no_servers')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })
})
