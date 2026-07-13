import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ComponentProps, ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentAuthorityStateSummary } from '@/lib/api-schema'
import { CAP_DEFAULT } from '@/lib/capabilities'

const mockPost = vi.fn()
const mockDelete = vi.fn()
const mockProjectServerCatalog = vi.hoisted(() => vi.fn())
const mockQueryClient = {}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: ({
    mutationFn,
    onError,
    onSuccess
  }: {
    mutationFn: (...args: unknown[]) => Promise<unknown>
    onError?: (error: unknown) => void
    onSuccess?: (data: unknown, variables: unknown) => void
  }) => ({
    isPending: false,
    mutate: async (variables?: unknown) => {
      try {
        const result = await mutationFn(variables)
        onSuccess?.(result, variables)
      } catch (error) {
        onError?.(error)
      }
    }
  }),
  useQueryClient: () => mockQueryClient
}))

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

vi.mock('@/lib/api-client', () => ({
  ApiError: class ApiError extends Error {},
  api: {
    delete: (path: string) => mockDelete(path),
    post: (path: string, body: unknown) => mockPost(path, body)
  }
}))

vi.mock('@/lib/server-catalog', () => ({ projectServerCatalog: mockProjectServerCatalog }))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, variant: _variant, ...props }: ComponentProps<'button'> & { variant?: string }) => (
    <button {...props}>{children}</button>
  )
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: ({
    checked,
    id,
    onCheckedChange
  }: {
    checked?: boolean
    id?: string
    onCheckedChange?: (checked: boolean) => void
  }) => (
    <input checked={checked} id={id} onChange={(event) => onCheckedChange?.(event.target.checked)} type="checkbox" />
  )
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  DialogBody: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogFooter: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

vi.mock('@/components/ui/alert-dialog', () => ({
  AlertDialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  AlertDialogAction: ({ children, variant: _variant, ...props }: ComponentProps<'button'> & { variant?: string }) => (
    <button {...props}>{children}</button>
  ),
  AlertDialogCancel: ({ children, ...props }: ComponentProps<'button'>) => <button {...props}>{children}</button>,
  AlertDialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  AlertDialogDescription: ({ children }: { children?: ReactNode }) => <p>{children}</p>,
  AlertDialogFooter: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  AlertDialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  AlertDialogTitle: ({ children }: { children?: ReactNode }) => <h3>{children}</h3>
}))

const { AgentReenrollmentDialog } = await import('./agent-reenrollment-dialog')

function makeServer(agentAuthority?: AgentAuthorityStateSummary) {
  const authority: AgentAuthorityStateSummary = agentAuthority ?? { outstanding_offer: null, status: 'claimed' }
  return {
    agent_authority: authority,
    capabilities: CAP_DEFAULT,
    id: 'srv-42',
    name: 'tokyo-vps-01'
  }
}

function offerResponse(id: string) {
  return {
    enrollment: {
      code: `plaintext-${id}`,
      code_prefix: 'plain',
      expires_at: '2030-01-01T00:00:00Z',
      id
    }
  }
}

describe('AgentReenrollmentDialog', () => {
  beforeEach(() => {
    mockPost.mockReset()
    mockDelete.mockReset()
    mockProjectServerCatalog.mockReset()
  })

  it('begins emergency re-enrollment by default', async () => {
    mockPost.mockResolvedValueOnce(offerResponse('offer-2'))
    render(<AgentReenrollmentDialog onOpenChange={vi.fn()} open server={makeServer()} />)

    fireEvent.click(screen.getByRole('button', { name: 'agent_reenrollment.generate' }))

    await waitFor(() => {
      expect(mockPost).toHaveBeenCalledWith('/api/servers/srv-42/agent-authority/re-enrollment', {
        mode: 'emergency'
      })
    })
    expect(await screen.findByText('plaintext-offer-2')).toBeInTheDocument()
  })

  it('submits graceful mode when emergency mode is disabled', async () => {
    mockPost.mockResolvedValueOnce(offerResponse('offer-3'))
    render(<AgentReenrollmentDialog onOpenChange={vi.fn()} open server={makeServer()} />)

    fireEvent.click(screen.getByLabelText('agent_reenrollment.emergency_mode'))
    fireEvent.click(screen.getByRole('button', { name: 'agent_reenrollment.generate' }))

    await waitFor(() => {
      expect(mockPost).toHaveBeenCalledWith('/api/servers/srv-42/agent-authority/re-enrollment', {
        mode: 'graceful'
      })
    })
  })

  it('replaces the exact visible outstanding offer', async () => {
    mockPost.mockResolvedValueOnce(offerResponse('offer-next'))
    render(
      <AgentReenrollmentDialog
        onOpenChange={vi.fn()}
        open
        server={makeServer({
          outstanding_offer: {
            code_prefix: 'abc123',
            created_at: '2025-01-01T00:00:00Z',
            expires_at: '2099-01-01T00:00:00Z',
            id: 'offer-current'
          },
          status: 'claimed'
        })}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'agent_reenrollment.replace_offer' }))

    await waitFor(() => {
      expect(mockPost).toHaveBeenCalledWith('/api/servers/srv-42/agent-authority/offers/offer-current/replace', {})
    })
  })

  it('revokes the exact visible outstanding offer', async () => {
    mockDelete.mockResolvedValueOnce({ already_revoked: false, offer_id: 'offer-current' })
    render(
      <AgentReenrollmentDialog
        onOpenChange={vi.fn()}
        open
        server={makeServer({
          outstanding_offer: {
            code_prefix: 'abc123',
            created_at: '2025-01-01T00:00:00Z',
            expires_at: '2099-01-01T00:00:00Z',
            id: 'offer-current'
          },
          status: 'claimed'
        })}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'agent_reenrollment.revoke_offer' }))

    await waitFor(() => {
      expect(mockDelete).toHaveBeenCalledWith('/api/servers/srv-42/agent-authority/offers/offer-current')
    })
  })

  it('renders an expired offer as terminal and removes its mutation actions', () => {
    render(
      <AgentReenrollmentDialog
        onOpenChange={vi.fn()}
        open
        server={makeServer({
          outstanding_offer: {
            code_prefix: 'expired',
            created_at: '2025-01-01T00:00:00Z',
            expires_at: '2025-01-01T00:10:00Z',
            id: 'offer-expired'
          },
          status: 'claimed'
        })}
      />
    )

    expect(screen.getByText('agent_reenrollment.expired_notice_title')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'agent_reenrollment.replace_offer' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'agent_reenrollment.revoke_offer' })).not.toBeInTheDocument()
  })

  it('requires destructive confirmation before revoking Agent authority', async () => {
    mockDelete.mockResolvedValueOnce({ changed: true, server_id: 'srv-42' })
    render(<AgentReenrollmentDialog onOpenChange={vi.fn()} open server={makeServer()} />)

    fireEvent.click(screen.getByRole('button', { name: 'agent_reenrollment.revoke_authority' }))
    const revokeButtons = screen.getAllByRole('button', { name: 'agent_reenrollment.revoke_authority' })
    fireEvent.click(revokeButtons.at(-1) ?? revokeButtons[0])

    await waitFor(() => {
      expect(mockDelete).toHaveBeenCalledWith('/api/servers/srv-42/agent-authority')
    })
    expect(mockProjectServerCatalog).toHaveBeenCalledWith(mockQueryClient, {
      authority: { outstanding_offer: null, status: 'unclaimed' },
      kind: 'agent_authority_changed',
      serverId: 'srv-42'
    })
  })
})
