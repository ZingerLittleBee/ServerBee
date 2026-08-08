import { cleanup, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const detailState = vi.hoisted(() => ({
  data: undefined as unknown,
  error: null as Error | null,
  isLoading: false
}))
const mockNavigate = vi.hoisted(() => vi.fn())

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    // The real Route class exposes the createFileRoute options on its
    // public `options` property (Route.options.component).
    options: config,
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
  useQuery: () => detailState
}))

vi.mock('react-i18next', () => ({
  // CountryFlag destructures `i18n` and reads `i18n.language`.
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'en' } })
}))

vi.mock('@/lib/api-client', () => ({
  api: { get: vi.fn() }
}))

vi.mock('@/hooks/use-public-status', () => ({
  usePublicStatusConfig: () => ({
    data: { enabled: true, show_server_detail: true, show_network: true }
  })
}))

vi.mock('@/components/status/server-detail-content', () => ({
  ServerDetailContent: () => <div data-testid="server-detail-content" />
}))

vi.mock('@/components/network/public-network-tab', () => ({
  PublicNetworkTab: () => <div data-testid="public-network-tab" />
}))

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

async function renderPage() {
  const { Route } = await import('./status.server.$serverId')
  const Page = requireRouteComponent(Route)
  return render(<Page />)
}

describe('PublicServerDetailPage', () => {
  beforeEach(() => {
    detailState.data = undefined
    detailState.error = null
    detailState.isLoading = false
    mockNavigate.mockClear()
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the generated status-server-detail skeleton while loading', async () => {
    detailState.isLoading = true
    const { container } = await renderPage()

    const skeleton = container.querySelector('[data-boneyard="status-server-detail"]')
    expect(skeleton).not.toBeNull()
    expect(skeleton?.getAttribute('aria-busy')).toBe('true')
    expect(screen.queryByTestId('server-detail-content')).toBeNull()
  })

  it('renders the detail content once loaded, without any skeleton container', async () => {
    detailState.data = { name: 'Web 01', online: true, country_code: 'US' }
    const { container } = await renderPage()

    expect(screen.getByText('Web 01')).toBeInTheDocument()
    expect(screen.getByTestId('server-detail-content')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })

  it('keeps the not-found branch free of skeleton markup', async () => {
    detailState.error = new Error('not found')
    const { container } = await renderPage()

    expect(screen.getByText('detail_not_found')).toBeInTheDocument()
    expect(container.querySelector('[data-boneyard]')).toBeNull()
  })
})
