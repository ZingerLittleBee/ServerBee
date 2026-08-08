import { cleanup, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const networkState = vi.hoisted(() => ({
  data: undefined as unknown,
  error: null as Error | null,
  isLoading: false
}))
const mockNavigate = vi.hoisted(() => vi.fn())

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useParams: () => ({ serverId: 'srv-1' }),
    useSearch: () => ({})
  }),
  Link: ({ children, to, ...rest }: { children: ReactNode; to: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
  useNavigate: () => mockNavigate
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: () => networkState
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@/lib/api-client', () => ({
  api: { get: vi.fn() }
}))

// show_server_detail=false + show_network=true makes this page the standalone
// network home, so it renders content instead of redirecting.
vi.mock('@/hooks/use-public-status', () => ({
  usePublicStatusConfig: () => ({
    data: { enabled: true, show_server_detail: false, show_network: true }
  })
}))

vi.mock('@/components/status/network-detail-content', () => ({
  NetworkDetailContent: () => <div data-testid="network-detail-content" />
}))

async function renderPage() {
  const { Route } = await import('./status.network.$serverId')
  const Page = (Route as { component: React.ComponentType }).component
  return render(<Page />)
}

describe('PublicNetworkDetailPage', () => {
  beforeEach(() => {
    networkState.data = undefined
    networkState.error = null
    networkState.isLoading = false
    mockNavigate.mockClear()
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the generated status-network-detail skeleton while loading', async () => {
    networkState.isLoading = true
    const { container } = await renderPage()

    const skeleton = container.querySelector('[data-boneyard="status-network-detail"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
    expect(screen.queryByTestId('network-detail-content')).toBeNull()
  })

  it('renders the network detail once loaded, without any skeleton container', async () => {
    networkState.data = {
      summary: { server_name: 'Edge 01', online: true, last_probe_at: null },
      anomalies: []
    }
    const { container } = await renderPage()

    expect(screen.getByText('Edge 01')).toBeInTheDocument()
    expect(screen.getByTestId('network-detail-content')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('keeps the not-found branch free of skeleton markup', async () => {
    networkState.error = new Error('not found')
    const { container } = await renderPage()

    expect(screen.getByText('server_not_found')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })
})
