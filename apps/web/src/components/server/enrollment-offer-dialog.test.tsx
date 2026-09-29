import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { EnrollmentOfferResponse, ServerResponse } from '@/lib/api-schema'
import { projectServerCatalog, useLiveServers } from '@/lib/server-catalog'
import { EnrollmentOfferDialog } from './enrollment-offer-dialog'

const mockPost = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

vi.mock('@/lib/api-client', () => ({
  ApiError: class ApiError extends Error {},
  api: {
    get: vi.fn(),
    post: (path: string, body: unknown) => mockPost(path, body)
  }
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  DialogBody: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogFooter: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

const SERVER_ID = 'srv-pending'

function offerResponse(n: number): EnrollmentOfferResponse {
  return {
    enrollment: {
      code: `code-${n}`,
      code_prefix: `pre${n}`,
      expires_at: '2099-01-01T00:00:00Z',
      id: `offer-${n}`
    }
  } as EnrollmentOfferResponse
}

/** Mirrors ServerCard: the offer the dialog replaces comes from the live catalog. */
function CatalogBackedDialog() {
  const { data } = useLiveServers()
  const server = data?.find((s) => s.id === SERVER_ID)
  return (
    <EnrollmentOfferDialog
      onOpenChange={vi.fn()}
      open
      outstandingOffer={server?.agent_authority?.outstanding_offer ?? null}
      serverId={SERVER_ID}
    />
  )
}

function seedPendingServer(queryClient: QueryClient, withOffer: boolean) {
  const offer = withOffer
    ? {
        code_prefix: 'pre0',
        created_at: '2026-01-01T00:00:00Z',
        expires_at: '2099-01-01T00:00:00Z',
        id: 'offer-0'
      }
    : null
  const server: ServerResponse = {
    agent_authority: { outstanding_offer: offer, status: 'unclaimed' },
    capabilities: 1852,
    country_code: null,
    cpu_cores: null,
    cpu_name: null,
    created_at: '2026-07-01T00:00:00Z',
    disk_total: null,
    effective_capabilities: 1852,
    features: [],
    geo_manual: false,
    group_id: null,
    has_token: false,
    hidden: false,
    id: SERVER_ID,
    mem_total: null,
    name: 'Pending',
    os: null,
    outstanding_enrollment: offer,
    protocol_version: 2,
    region: null,
    swap_total: null,
    temporary: [],
    updated_at: '2026-07-01T00:00:00Z',
    weight: 100
  }
  projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [server] })
}

describe('EnrollmentOfferDialog', () => {
  beforeEach(() => {
    mockPost.mockReset()
    let n = 0
    mockPost.mockImplementation(() => {
      n += 1
      return Promise.resolve(offerResponse(n))
    })
  })

  async function renderOpenDialog(withOffer: boolean) {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false }, queries: { retry: false } } })
    seedPendingServer(queryClient, withOffer)
    render(
      <QueryClientProvider client={queryClient}>
        <CatalogBackedDialog />
      </QueryClientProvider>
    )
    await screen.findByText('code-1')
    // Give any remount-driven re-issue loop time to fire.
    await new Promise((resolve) => setTimeout(resolve, 200))
  }

  it('issues exactly one replacement per open, even after the catalog picks up the new offer', async () => {
    await renderOpenDialog(true)

    expect(mockPost).toHaveBeenCalledTimes(1)
    expect(mockPost).toHaveBeenCalledWith(`/api/servers/${SERVER_ID}/agent-authority/offers/offer-0/replace`, {})
    await waitFor(() => expect(screen.getByText('code-1')).toBeTruthy())
  })

  it('issues exactly one offer per open when the server had none', async () => {
    await renderOpenDialog(false)

    expect(mockPost).toHaveBeenCalledTimes(1)
    expect(mockPost).toHaveBeenCalledWith(`/api/servers/${SERVER_ID}/agent-authority/offers`, {})
    await waitFor(() => expect(screen.getByText('code-1')).toBeTruthy())
  })
})
