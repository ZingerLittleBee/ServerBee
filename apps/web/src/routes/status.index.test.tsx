import { cleanup, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { STATUS_LAYOUT_STORAGE_KEY } from '@/hooks/use-status-layout'

const serversState = vi.hoisted(() => ({
  data: undefined as unknown,
  error: null as Error | null,
  isLoading: false
}))
const configState = vi.hoisted(() => ({ defaultLayout: 'grid' as 'grid' | 'list' }))

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
    data: { enabled: true, show_server_detail: true, default_layout: configState.defaultLayout }
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
    configState.defaultLayout = 'grid'
    localStorage.clear()
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the grid skeleton while loading with the default layout', async () => {
    serversState.isLoading = true
    const { container } = await renderPage()

    const skeleton = container.querySelector('[data-boneyard="status-overview-grid"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
    expect(container.querySelector('[data-boneyard="status-overview-list"]')).toBeNull()
  })

  it('renders the list skeleton while loading when the user persisted list layout', async () => {
    localStorage.setItem(STATUS_LAYOUT_STORAGE_KEY, 'list')
    serversState.isLoading = true
    const { container } = await renderPage()

    expect(container.querySelector('[data-boneyard="status-overview-list"]')).not.toBeNull()
    expect(container.querySelector('[data-boneyard="status-overview-grid"]')).toBeNull()
  })

  it('renders the list skeleton while loading when the config default is list', async () => {
    configState.defaultLayout = 'list'
    serversState.isLoading = true
    const { container } = await renderPage()

    expect(container.querySelector('[data-boneyard="status-overview-list"]')).not.toBeNull()
  })

  it('does not jump layouts when data arrives after a list skeleton', async () => {
    localStorage.setItem(STATUS_LAYOUT_STORAGE_KEY, 'list')
    serversState.isLoading = true
    const { Route } = await import('./status.index')
    const Page = (Route as { component: React.ComponentType }).component
    const { container, rerender } = render(<Page />)

    expect(container.querySelector('[data-boneyard="status-overview-list"]')).not.toBeNull()

    serversState.isLoading = false
    serversState.data = [{ id: 'srv-1' }, { id: 'srv-2' }]
    rerender(<Page />)

    // The loaded page renders the same list layout the skeleton previewed.
    expect(container.querySelectorAll('[data-testid="summary-row"]')).toHaveLength(2)
    expect(container.querySelectorAll('[data-testid="summary-card"]')).toHaveLength(0)
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('renders server cards once loaded in grid layout, without any skeleton container', async () => {
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
