import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { OutstandingEnrollmentSummary, ServerResponse } from '@/lib/api-schema'
import { CAP_DEFAULT } from '@/lib/capabilities'

const mockPost = vi.fn()
const mockDelete = vi.fn()
const mockSetQueryData = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key
  })
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: ({
    mutationFn,
    onSuccess,
    onError
  }: {
    mutationFn: (...args: unknown[]) => Promise<unknown>
    onError?: (err: unknown) => void
    onSuccess?: (data: unknown, variables: unknown) => void
  }) => ({
    error: null,
    isPending: false,
    mutate: async (vars: unknown) => {
      try {
        const result = await mutationFn(vars)
        onSuccess?.(result, vars)
      } catch (err) {
        onError?.(err)
      }
    }
  }),
  useQueryClient: () => ({ setQueryData: mockSetQueryData })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/lib/api-client', () => ({
  ApiError: class ApiError extends Error {
    status: number
    code?: string
    constructor(message: string, status: number, code?: string) {
      super(message)
      this.status = status
      this.code = code
    }
  },
  api: {
    get: vi.fn(),
    post: (path: string, body: unknown) => mockPost(path, body),
    delete: (path: string) => mockDelete(path)
  }
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({
    children,
    type,
    ...props
  }: { children?: ReactNode; type?: 'button' | 'submit' | 'reset' } & Record<string, unknown>) => (
    <button type={type ?? 'button'} {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: ({
    checked,
    onCheckedChange,
    id
  }: {
    checked?: boolean
    id?: string
    onCheckedChange?: (checked: boolean) => void
  }) => (
    <input checked={checked} id={id} onChange={(event) => onCheckedChange?.(event.target.checked)} type="checkbox" />
  )
}))

const TITLE_HEADING_RE = /recover_agent\.title/
const CODE_PREFIX_RE = /abc123/

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children?: ReactNode; open?: boolean }) => (open ? <div>{children}</div> : null),
  DialogBody: ({ children, className }: { children?: ReactNode; className?: string }) => (
    <div className={className} data-slot="dialog-body">
      {children}
    </div>
  ),
  DialogContent: ({ children, className }: { children?: ReactNode; className?: string }) => (
    <div className={className} data-testid="dialog-content">
      {children}
    </div>
  ),
  DialogFooter: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-footer">{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

const { RecoverAgentDialog } = await import('./recover-agent-dialog')

function makeServer(overrides: Partial<ServerResponse> = {}): ServerResponse {
  return {
    id: 'srv-42',
    name: 'tokyo-vps-01',
    capabilities: CAP_DEFAULT,
    has_token: true,
    hidden: false,
    features: [],
    weight: 0,
    protocol_version: 1,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
    outstanding_enrollment: null,
    ...overrides
  } as ServerResponse
}

describe('RecoverAgentDialog', () => {
  beforeEach(() => {
    mockPost.mockReset()
    mockDelete.mockReset()
    mockSetQueryData.mockReset()
  })

  it('renders the server name in a read-only header (not as an input)', () => {
    render(<RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer()} />)
    expect(screen.getByRole('heading', { name: TITLE_HEADING_RE })).toBeInTheDocument()
    // Server name displayed somewhere as read-only text
    expect(screen.getByText('tokyo-vps-01')).toBeInTheDocument()
    // No input for the name
    expect(screen.queryByLabelText('recover_agent.server_name_label')).not.toBeInTheDocument()
  })

  it('shows the TTL tip but no TTL selector', () => {
    render(<RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer()} />)
    expect(screen.getByText('recover_agent.ttl_tip')).toBeInTheDocument()
  })

  it('defaults revoke_immediately to checked and shows the warning text', () => {
    render(<RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer()} />)
    const checkbox = screen.getByLabelText('recover_agent.revoke_immediately') as HTMLInputElement
    expect(checkbox.checked).toBe(true)
    expect(screen.getByText('recover_agent.revoke_warning')).toBeInTheDocument()
  })

  it('POSTs revoke_immediately=true by default and transitions to install-command view on 200', async () => {
    mockPost.mockResolvedValueOnce({
      enrollment: {
        id: 'enr-9',
        code: 'plaintext-recover-code',
        code_prefix: 'plaint',
        expires_at: '2030-01-01T00:00:00Z'
      }
    })

    render(<RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer()} />)

    const submit = screen.getByRole('button', { name: 'recover_agent.generate' })
    fireEvent.click(submit)

    await waitFor(() => expect(mockPost).toHaveBeenCalledTimes(1))
    const [path, body] = mockPost.mock.calls[0]
    expect(path).toBe('/api/servers/srv-42/recover')
    expect(body).toEqual({ revoke_immediately: true })

    await waitFor(() => {
      expect(screen.getByText('plaintext-recover-code')).toBeInTheDocument()
    })
    expect(screen.getByText('add_server.shown_once_warning')).toBeInTheDocument()
    expect(mockSetQueryData).toHaveBeenCalledWith(['servers'], expect.any(Function))
    // The updater should patch outstanding_enrollment and (since revoke_immediately
    // defaulted to true) flip has_token / online to false.
    const updater = mockSetQueryData.mock.calls[0][1] as (
      prev: Record<string, unknown>[] | undefined
    ) => Record<string, unknown>[] | undefined
    const patched = updater([{ id: 'srv-42', has_token: true, online: true, outstanding_enrollment: null }])
    expect(patched?.[0]).toMatchObject({
      has_token: false,
      online: false,
      outstanding_enrollment: { id: 'enr-9', code_prefix: 'plaint' }
    })
  })

  it('toggling revoke_immediately off changes the submitted body to false', async () => {
    mockPost.mockResolvedValueOnce({
      enrollment: {
        id: 'enr-10',
        code: 'another-code',
        code_prefix: 'anothe',
        expires_at: '2030-01-01T00:00:00Z'
      }
    })

    render(<RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer()} />)
    const checkbox = screen.getByLabelText('recover_agent.revoke_immediately') as HTMLInputElement
    fireEvent.click(checkbox)
    expect(checkbox.checked).toBe(false)

    const submit = screen.getByRole('button', { name: 'recover_agent.generate' })
    fireEvent.click(submit)

    await waitFor(() => expect(mockPost).toHaveBeenCalledTimes(1))
    const [, body] = mockPost.mock.calls[0]
    expect(body).toEqual({ revoke_immediately: false })
  })

  it('renders the outstanding-enrollment notice + Revoke button when outstanding_enrollment is set', async () => {
    mockDelete.mockResolvedValueOnce(undefined)

    const outstanding: OutstandingEnrollmentSummary = {
      id: 'enr-out-1',
      code_prefix: 'abc123',
      created_at: '2025-01-01T00:00:00Z',
      expires_at: '2099-01-01T00:00:00Z'
    }

    render(
      <RecoverAgentDialog onOpenChange={vi.fn()} open server={makeServer({ outstanding_enrollment: outstanding })} />
    )

    // The form is NOT rendered: no Generate button, no revoke_immediately checkbox.
    expect(screen.queryByRole('button', { name: 'recover_agent.generate' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('recover_agent.revoke_immediately')).not.toBeInTheDocument()

    // Notice block is rendered with the prefix.
    expect(screen.getByText('recover_agent.outstanding_notice_title')).toBeInTheDocument()
    expect(screen.getByText(CODE_PREFIX_RE)).toBeInTheDocument()

    const revoke = screen.getByRole('button', { name: 'recover_agent.revoke' })
    fireEvent.click(revoke)

    await waitFor(() => expect(mockDelete).toHaveBeenCalledTimes(1))
    expect(mockDelete).toHaveBeenCalledWith('/api/agent/enrollments/enr-out-1')
    await waitFor(() => {
      expect(mockSetQueryData).toHaveBeenCalledWith(['servers'], expect.any(Function))
    })
    // The updater should clear outstanding_enrollment on the affected row.
    const updater = mockSetQueryData.mock.calls[0][1] as (
      prev: Record<string, unknown>[] | undefined
    ) => Record<string, unknown>[] | undefined
    const patched = updater([{ id: 'srv-42', outstanding_enrollment: { id: 'enr-out-1', code_prefix: 'abc123' } }])
    expect(patched?.[0]).toMatchObject({ outstanding_enrollment: null })
  })
})
