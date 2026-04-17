import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerResponse } from '@/lib/api-schema'

const mockInvalidateQueries = vi.fn()
const mockSetQueryData = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key
  })
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: () => ({
    error: null,
    isPending: false,
    mutateAsync: vi.fn()
  }),
  useQuery: () => ({
    data: [],
    isLoading: false
  }),
  useQueryClient: () => ({
    invalidateQueries: mockInvalidateQueries,
    setQueryData: mockSetQueryData
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/hooks/use-server-tags', () => ({
  useServerTags: () => ({
    data: ['prod', 'edge']
  }),
  useUpdateServerTags: () => ({
    isPending: false,
    mutateAsync: vi.fn()
  })
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: vi.fn(),
    put: vi.fn()
  }
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button {...props}>{children}</button>
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

const { ServerEditDialog } = await import('./server-edit-dialog')

const server: ServerResponse = {
  billing_cycle: null,
  billing_start_day: null,
  capabilities: 56,
  created_at: '2026-04-18T00:00:00Z',
  currency: 'USD',
  expired_at: null,
  features: [],
  group_id: null,
  hidden: false,
  id: 'server-1',
  name: 'Tokyo Edge',
  price: null,
  protocol_version: 2,
  public_remark: null,
  remark: null,
  traffic_limit: null,
  traffic_limit_type: 'sum',
  updated_at: '2026-04-18T00:00:00Z',
  weight: 100
}

describe('ServerEditDialog', () => {
  it('keeps the dialog body inside a scrollable flex column layout', () => {
    render(<ServerEditDialog onClose={vi.fn()} open server={server} />)

    const saveButton = screen.getByRole('button', { name: 'common:save' })
    const form = saveButton.closest('form')

    expect(form).toBeInTheDocument()
    expect(form).toHaveClass('flex')
    expect(form).toHaveClass('min-h-0')
    expect(form).toHaveClass('flex-1')
    expect(form).toHaveClass('flex-col')
    expect(form).not.toHaveClass('contents')
    expect(form?.querySelector('[data-slot="dialog-body"]')).toBeInTheDocument()
  })
})
