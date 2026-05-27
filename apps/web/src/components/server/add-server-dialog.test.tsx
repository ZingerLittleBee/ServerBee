import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockPost = vi.fn()
const mockGet = vi.fn()
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
    onSuccess?: (data: unknown) => void
  }) => ({
    error: null,
    isPending: false,
    mutate: async (vars: unknown) => {
      try {
        const result = await mutationFn(vars)
        onSuccess?.(result)
      } catch (err) {
        onError?.(err)
      }
    }
  }),
  useQuery: () => ({ data: [], isLoading: false }),
  useQueryClient: () => ({ setQueryData: mockSetQueryData })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: (path: string) => mockGet(path),
    post: (path: string, body: unknown) => mockPost(path, body)
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

vi.mock('@/components/ui/calendar', () => ({
  Calendar: () => <div data-testid="calendar" />
}))

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: ({ checked, onCheckedChange }: { checked?: boolean; onCheckedChange?: (checked: boolean) => void }) => (
    <input checked={checked} onChange={(event) => onCheckedChange?.(event.target.checked)} type="checkbox" />
  )
}))

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

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />
}))

vi.mock('@/components/ui/popover', () => ({
  Popover: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  PopoverContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  PopoverTrigger: ({ children }: { children?: ReactNode }) => <button type="button">{children}</button>
}))

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectValue: () => <span />
}))

const { AddServerDialog } = await import('./add-server-dialog')

describe('AddServerDialog', () => {
  beforeEach(() => {
    mockPost.mockReset()
    mockGet.mockReset()
    mockSetQueryData.mockReset()
  })

  it('POSTs to /api/servers with the form payload and transitions to the install-command view on success', async () => {
    mockPost.mockResolvedValueOnce({
      server_id: 'srv-123',
      enrollment: {
        id: 'enr-1',
        code: 'plaintext-code-shown-once',
        code_prefix: 'plaint',
        expires_at: '2030-01-01T00:00:00Z'
      }
    })
    mockGet.mockResolvedValueOnce([])

    render(<AddServerDialog onClose={vi.fn()} open />)

    const nameInput = screen.getByLabelText('add_server.name_label') as HTMLInputElement
    fireEvent.change(nameInput, { target: { value: 'tokyo-vps-01' } })

    const submit = screen.getByRole('button', { name: 'add_server.generate' })
    fireEvent.click(submit)

    await waitFor(() => expect(mockPost).toHaveBeenCalledTimes(1))

    const [path, body] = mockPost.mock.calls[0]
    expect(path).toBe('/api/servers')
    expect(body).toMatchObject({ name: 'tokyo-vps-01' })
    expect((body as Record<string, unknown>).group_id).toBeUndefined()
    expect((body as Record<string, unknown>).caps).toBeUndefined()

    await waitFor(() => {
      expect(screen.getByText('plaintext-code-shown-once')).toBeInTheDocument()
    })

    expect(screen.getByText('add_server.shown_once_warning')).toBeInTheDocument()
    await waitFor(() => expect(mockGet).toHaveBeenCalledWith('/api/servers'))
    expect(mockSetQueryData).toHaveBeenCalledWith(['servers'], expect.any(Function))
    expect(screen.queryByRole('button', { name: 'add_server.generate' })).not.toBeInTheDocument()
  })

  it('disables the submit button when name is empty', () => {
    render(<AddServerDialog onClose={vi.fn()} open />)
    const submit = screen.getByRole('button', { name: 'add_server.generate' }) as HTMLButtonElement
    expect(submit.disabled).toBe(true)
  })

  it('shows the TTL tip without a selector', () => {
    render(<AddServerDialog onClose={vi.fn()} open />)
    expect(screen.getByText('add_server.ttl_tip')).toBeInTheDocument()
    expect(screen.queryByText('add_server.validity_10m')).not.toBeInTheDocument()
    expect(screen.queryByText('add_server.validity_1h')).not.toBeInTheDocument()
  })
})
