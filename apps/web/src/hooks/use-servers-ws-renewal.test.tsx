import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AlertListWidget } from '@/components/dashboard/widgets/alert-list'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import type { CostOverviewResponse, ServerCostInsights, ServerResponse } from '@/lib/api-schema'
import { useLiveServers, useServerDetail, useServerList } from '@/lib/server-catalog'
import { useCostInsights, useCostOverview } from './use-cost'
import { useServersWs } from './use-servers-ws'

// Stub the network boundary; the hook, WsClient, catalog and cost hooks remain real.
class BrowserSocket {
  static instances: BrowserSocket[] = []
  readonly url: string
  onmessage: ((event: MessageEvent) => void) | null = null
  close() {
    this.onmessage = null
  }
  constructor(url: string) {
    this.url = url
    BrowserSocket.instances.push(this)
  }
  receive(message: unknown) {
    this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(message) }))
  }
}

const server: ServerResponse = {
  agent_authority: { outstanding_offer: null, status: 'unclaimed' },
  capabilities: 0,
  created_at: '2026-01-01T00:00:00Z',
  expired_at: '2026-01-31T23:59:59Z',
  features: [],
  geo_manual: false,
  has_token: false,
  hidden: false,
  id: 'srv-1',
  name: 'Offline monthly server',
  protocol_version: 2,
  updated_at: '2026-01-01T00:00:00Z',
  weight: 0
}

function setup() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } }
  })
  let currentServer = server
  let expired = true
  const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation((input, options) => {
    const path = String(input)
    let data: ServerResponse | ServerResponse[] | CostOverviewResponse | ServerCostInsights
    if (path === '/api/servers/srv-1' && options?.method === 'PUT') {
      currentServer = { ...server, expired_at: '2026-03-31T23:59:59Z' }
      expired = false
      data = currentServer
    } else if (path === '/api/server-groups' || path === '/api/servers/srv-1/tags') {
      data = []
    } else if (path === '/api/alert-events?limit=10') {
      return Promise.resolve(
        Response.json({
          data: [
            {
              rule_id: 'rule-1',
              rule_name: expired ? 'Prior occurrence' : 'Superseded occurrence',
              server_id: 'srv-1',
              server_name: server.name,
              status: expired ? 'firing' : 'superseded',
              event_at: '2026-01-31T00:00:00Z',
              resolved_at: null,
              count: 1
            }
          ]
        })
      )
    } else if (path === '/api/servers') {
      data = [currentServer]
    } else if (path === '/api/servers/srv-1') {
      data = currentServer
    } else if (path === '/api/cost/overview') {
      data = {
        currencies: [],
        servers: [
          { server_id: 'srv-1', name: server.name, configured: true, advisories: expired ? ['expired_billing'] : [] }
        ]
      }
    } else if (path.endsWith('/cost-insights')) {
      data = {
        server_id: path.split('/')[3],
        configured: true,
        advisories: expired ? ['expired_billing'] : []
      }
    } else {
      throw new Error(`Unexpected request: ${path}`)
    }
    return Promise.resolve(Response.json({ data }))
  })
  vi.stubGlobal('WebSocket', BrowserSocket)
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return {
    queryClient,
    fetchSpy,
    wrapper,
    advance() {
      currentServer = { ...server, expired_at: '2026-02-28T23:59:59Z' }
      expired = false
    },
    receive(message: unknown) {
      act(() => BrowserSocket.instances[0].receive(message))
    }
  }
}

afterEach(() => {
  cleanup()
  BrowserSocket.instances = []
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('renewal catalog refresh through the browser WebSocket', () => {
  it('refreshes current detail, list, dashboard and cost advisories after an affected catalog change', async () => {
    const fixture = setup()
    const { result } = renderHook(
      () => {
        useServersWs()
        return {
          list: useServerList(),
          detail: useServerDetail('srv-1'),
          dashboard: useLiveServers(),
          overview: useCostOverview(),
          insights: useCostInsights('srv-1'),
          unrelated: useCostInsights('srv-2')
        }
      },
      { wrapper: fixture.wrapper }
    )
    await waitFor(() => expect(result.current.insights.data?.advisories).toEqual(['expired_billing']))
    await waitFor(() => expect(result.current.detail.data?.expired_at).toBe('2026-01-31T23:59:59Z'))
    await waitFor(() => expect(result.current.unrelated.data?.advisories).toEqual(['expired_billing']))

    fixture.advance()
    fixture.receive({ type: 'server_catalog_changed', server_ids: ['srv-1'] })

    await waitFor(() => expect(result.current.detail.data?.expired_at).toBe('2026-02-28T23:59:59Z'))
    await waitFor(() => expect(result.current.list.data?.[0].expired_at).toBe('2026-02-28T23:59:59Z'))
    await waitFor(() => expect(result.current.overview.data?.servers[0].advisories).toEqual([]))
    await waitFor(() => expect(result.current.insights.data?.advisories).toEqual([]))
    expect(result.current.dashboard.data?.[0].name).toBe(server.name)
    expect(result.current.dashboard.data?.[0]).not.toHaveProperty('expired_at')
    expect(result.current.dashboard.data?.[0]).not.toHaveProperty('renewal')
    expect(result.current.unrelated.data?.advisories).toEqual(['expired_billing'])
    expect(fixture.fetchSpy.mock.calls.filter(([path]) => path === '/api/servers/srv-2/cost-insights')).toHaveLength(1)
  })

  it('refreshes REST deadlines and every cached cost advisory on a full sync after a missed catalog event', async () => {
    const fixture = setup()
    const { result } = renderHook(
      () => {
        useServersWs()
        return {
          list: useServerList(),
          detail: useServerDetail('srv-1'),
          overview: useCostOverview(),
          insights: useCostInsights('srv-1'),
          otherInsights: useCostInsights('srv-2')
        }
      },
      { wrapper: fixture.wrapper }
    )
    await waitFor(() => expect(result.current.detail.data?.expired_at).toBe('2026-01-31T23:59:59Z'))
    await waitFor(() => expect(result.current.otherInsights.data?.advisories).toEqual(['expired_billing']))

    fixture.advance()
    fixture.receive({ type: 'full_sync', servers: fixture.queryClient.getQueryData(['server-catalog', 'live']) })

    await waitFor(() => expect(result.current.detail.data?.expired_at).toBe('2026-02-28T23:59:59Z'))
    await waitFor(() => expect(result.current.overview.data?.servers[0].advisories).toEqual([]))
    await waitFor(() => expect(result.current.insights.data?.advisories).toEqual([]))
    await waitFor(() => expect(result.current.otherInsights.data?.advisories).toEqual([]))
    expect(fixture.fetchSpy).toHaveBeenCalledWith('/api/servers', expect.any(Object))
  })

  it('refreshes independently cached cost advisories after saving a manual date through the billing editor', async () => {
    const fixture = setup()
    Element.prototype.getAnimations = () => []
    const { result } = renderHook(() => ({ overview: useCostOverview(), insights: useCostInsights('srv-1') }), {
      wrapper: fixture.wrapper
    })
    await waitFor(() => expect(result.current.insights.data?.advisories).toEqual(['expired_billing']))
    await waitFor(() => expect(result.current.overview.data?.servers[0].advisories).toEqual(['expired_billing']))
    render(<ServerEditDialog onClose={vi.fn()} open server={server} />, { wrapper: fixture.wrapper })
    fireEvent.change(screen.getByLabelText('Expiration date'), { target: { value: '2026-03-31' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(fixture.fetchSpy).toHaveBeenCalledWith(
        '/api/servers/srv-1',
        expect.objectContaining({
          method: 'PUT',
          body: expect.stringContaining('"expiry_date":"2026-03-31"')
        })
      )
    )
    await waitFor(() => expect(result.current.overview.data?.servers[0].advisories).toEqual([]))
    await waitFor(() => expect(result.current.insights.data?.advisories).toEqual([]))
  })

  it('refreshes dashboard alert occurrences when advancement supersedes the old target', async () => {
    const fixture = setup()
    renderHook(() => useServersWs(), { wrapper: fixture.wrapper })
    render(<AlertListWidget config={{}} servers={[]} />, { wrapper: fixture.wrapper })
    expect(await screen.findByText('Prior occurrence')).toBeInTheDocument()

    fixture.advance()
    fixture.receive({ type: 'server_catalog_changed', server_ids: ['srv-1'] })

    expect(await screen.findByText('Superseded occurrence')).toBeInTheDocument()
  })

  it('describes a superseded renewal occurrence without showing recovery', async () => {
    const fixture = setup()
    fixture.advance()
    render(<AlertListWidget config={{}} servers={[]} />, { wrapper: fixture.wrapper })

    expect(await screen.findByLabelText('Superseded by the next renewal deadline')).toBeInTheDocument()
    expect(screen.queryByLabelText('Resolved')).not.toBeInTheDocument()
  })

  it('refreshes cost views opened after a reconnect even while their cached results were inactive and fresh', async () => {
    const fixture = setup()
    renderHook(
      () => {
        useServersWs()
        return useServerList()
      },
      { wrapper: fixture.wrapper }
    )
    const costs = renderHook(() => ({ overview: useCostOverview(), insights: useCostInsights('srv-2') }), {
      wrapper: fixture.wrapper
    })
    await waitFor(() => expect(costs.result.current.insights.data?.advisories).toEqual(['expired_billing']))
    await waitFor(() => expect(costs.result.current.overview.data?.servers[0].advisories).toEqual(['expired_billing']))
    costs.unmount()

    fixture.advance()
    fixture.receive({ type: 'full_sync', servers: fixture.queryClient.getQueryData(['server-catalog', 'live']) })
    const reopened = renderHook(() => ({ overview: useCostOverview(), insights: useCostInsights('srv-2') }), {
      wrapper: fixture.wrapper
    })

    await waitFor(() => expect(reopened.result.current.insights.data?.advisories).toEqual([]))
    await waitFor(() => expect(reopened.result.current.overview.data?.servers[0].advisories).toEqual([]))
  })

  it.each([null, 'srv-1', [], ['srv-1', null], [1], ['']])('ignores malformed catalog identifiers %j', (serverIds) => {
    const fixture = setup()
    renderHook(() => useServersWs(), { wrapper: fixture.wrapper })
    fixture.receive({ type: 'server_catalog_changed', server_ids: serverIds })

    expect(fixture.fetchSpy).not.toHaveBeenCalled()
  })
})
