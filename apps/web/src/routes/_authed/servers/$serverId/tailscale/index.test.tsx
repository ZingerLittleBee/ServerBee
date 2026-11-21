import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerTailscaleStatusResponse } from '@/lib/api-schema'

const mockRefetch = vi.fn()
const mockUseServerDetail = vi.fn()
const mockUseQuery = vi.fn()
const mockCapEnabled = vi.fn<(_eff?: unknown, _cfg?: unknown, _bit?: unknown) => boolean>(() => true)

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to }: { children?: ReactNode; to?: string }) => <a href={to ?? '/'}>{children}</a>,
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useParams: () => ({ serverId: 'srv-1' })
  })
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: (opts: unknown) => mockUseQuery(opts)
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      if (key === 'peers.mode.derpRegion') {
        return `DERP (${opts?.region})`
      }
      if (key === 'overview.derpRegion') {
        return `Home DERP: ${opts?.region}`
      }
      if (key === 'overview.keyExpiresInDays') {
        return `in ${opts?.days}d`
      }
      return key
    }
  })
}))

vi.mock('@/lib/capabilities', () => ({
  CAP_TAILSCALE: 2048,
  getEffectiveCapabilityEnabled: (eff: unknown, cfg: unknown, bit: unknown) => mockCapEnabled(eff, cfg, bit)
}))

vi.mock('@/lib/server-catalog', () => ({
  useServerDetail: (id: string) => mockUseServerDetail(id)
}))

const { ServerTailscalePage } = await import('./index')

const SAMPLE_STATUS: ServerTailscaleStatusResponse = {
  available: true,
  online: true,
  cached: false,
  tailscale_ipv4: '100.64.0.10',
  tailscale_ipv6: 'fd7a:115c:a1e0::10',
  tailscale_dns_name: 'edge-1.tailnet.ts.net',
  tailscale_tailnet: 'tailnet.ts.net',
  status: {
    summary: {
      backend_state: 'Running',
      tailscale_ips: ['100.64.0.10', 'fd7a:115c:a1e0::10'],
      dns_name: 'edge-1.tailnet.ts.net',
      tailnet_name: 'tailnet.ts.net',
      version: '1.76.1',
      hostname: 'edge-1',
      online: true,
      exit_node: true,
      exit_node_option: true,
      key_expiry: '2030-01-01T00:00:00Z'
    },
    advertised_routes: ['10.10.0.0/24'],
    tags: ['tag:prod'],
    derp_region: 'tok',
    health_warnings: ['DNS configuration degraded'],
    collected_at: '2026-11-21T00:00:00Z',
    peers: [
      {
        id: 'peer-direct',
        host_name: 'db-primary',
        dns_name: 'db-primary.tailnet.ts.net',
        os: 'linux',
        tailscale_ips: ['100.64.0.20'],
        online: true,
        active: true,
        connection_mode: 'direct',
        cur_addr: '203.0.113.5:41641',
        relay: 'tok',
        rx_bytes: 2048,
        tx_bytes: 4096,
        last_handshake: new Date().toISOString(),
        last_seen: new Date().toISOString(),
        exit_node: false,
        exit_node_option: false,
        tags: []
      },
      {
        id: 'peer-derp',
        host_name: 'laptop-mac',
        dns_name: 'laptop-mac.tailnet.ts.net',
        os: 'macOS',
        tailscale_ips: ['100.64.0.30'],
        online: true,
        active: true,
        connection_mode: 'derp',
        cur_addr: null,
        relay: 'tok',
        rx_bytes: 1024,
        tx_bytes: 512,
        last_handshake: '0001-01-01T00:00:00Z',
        last_seen: null,
        exit_node: false,
        exit_node_option: true,
        tags: []
      },
      {
        id: 'peer-relay',
        host_name: 'branch-gw',
        dns_name: 'branch-gw.tailnet.ts.net',
        os: 'linux',
        tailscale_ips: ['100.64.0.40'],
        online: true,
        active: true,
        connection_mode: 'peer_relay',
        cur_addr: null,
        relay: null,
        rx_bytes: 512,
        tx_bytes: 512,
        last_handshake: null,
        last_seen: null,
        exit_node: false,
        exit_node_option: false,
        tags: []
      },
      {
        id: 'peer-idle',
        host_name: 'cold-standby',
        dns_name: 'cold-standby.tailnet.ts.net',
        os: 'linux',
        tailscale_ips: ['100.64.0.50'],
        online: false,
        active: false,
        connection_mode: 'idle',
        cur_addr: null,
        relay: null,
        rx_bytes: 0,
        tx_bytes: 0,
        last_handshake: null,
        last_seen: null,
        exit_node: false,
        exit_node_option: false,
        tags: []
      }
    ]
  }
}

describe('ServerTailscalePage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCapEnabled.mockReturnValue(true)
    mockUseServerDetail.mockReturnValue({
      data: { id: 'srv-1', name: 'Tokyo Edge', capabilities: 3900, effective_capabilities: 3900 },
      isLoading: false
    })
    mockUseQuery.mockReturnValue({
      data: SAMPLE_STATUS,
      isLoading: false,
      isFetching: false,
      refetch: mockRefetch
    })
  })

  it('renders overview cards, health warnings, routes, and peer connection modes', () => {
    render(<ServerTailscalePage />)

    expect(screen.getByText('Running')).toBeInTheDocument()
    expect(screen.getByText('100.64.0.10')).toBeInTheDocument()
    expect(screen.getByText('fd7a:115c:a1e0::10')).toBeInTheDocument()
    expect(screen.getByText('edge-1.tailnet.ts.net')).toBeInTheDocument()
    expect(screen.getByText('Home DERP: tok')).toBeInTheDocument()
    expect(screen.getByText('10.10.0.0/24')).toBeInTheDocument()
    expect(screen.getByText('tag:prod')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('DNS configuration degraded')

    // Peer connection modes: Direct + cur_addr, DERP (tok), Peer Relay, Idle
    expect(screen.getByText('db-primary')).toBeInTheDocument()
    expect(screen.getByText('peers.mode.direct')).toBeInTheDocument()
    expect(screen.getByText('203.0.113.5:41641')).toBeInTheDocument()
    expect(screen.getByText('DERP (tok)')).toBeInTheDocument()
    expect(screen.getByText('peers.mode.peerRelay')).toBeInTheDocument()
    expect(screen.getByText('peers.mode.idle')).toBeInTheDocument()
    expect(screen.getAllByText('peers.never').length).toBeGreaterThan(0)
  })

  it('filters peers by search query and online status', () => {
    render(<ServerTailscalePage />)

    const searchInput = screen.getByLabelText('peers.searchPlaceholder')
    fireEvent.change(searchInput, { target: { value: 'laptop-mac' } })

    expect(screen.getByText('laptop-mac')).toBeInTheDocument()
    expect(screen.queryByText('db-primary')).not.toBeInTheDocument()
  })

  it('shows capability disabled state when CAP_TAILSCALE is off', () => {
    mockCapEnabled.mockReturnValue(false)
    render(<ServerTailscalePage />)

    expect(screen.getByText('common:capability_disabled_title')).toBeInTheDocument()
    expect(screen.queryByText('Running')).not.toBeInTheDocument()
  })

  it('shows unavailable empty state when tailscaled is not available', () => {
    mockUseQuery.mockReturnValue({
      data: { available: false, online: true, cached: false, status: null },
      isLoading: false,
      isFetching: false,
      refetch: mockRefetch
    })
    render(<ServerTailscalePage />)

    expect(screen.getByText('tailscale:notAvailable.title')).toBeInTheDocument()
  })
})
