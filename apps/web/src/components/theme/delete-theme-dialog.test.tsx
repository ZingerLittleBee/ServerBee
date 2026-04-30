import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockDeleteMutate = vi.fn()
let mockReferences: { admin: boolean; status_pages: { id: string; name: string }[] } | undefined

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (typeof options?.name === 'string') {
        return `${key}:${options.name}`
      }
      return key
    }
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/api/themes', () => ({
  useDeleteTheme: () => ({
    isPending: false,
    mutate: mockDeleteMutate
  }),
  useThemeReferences: () => ({
    data: mockReferences,
    isLoading: false
  })
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogFooter: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

const { DeleteThemeDialog } = await import('./delete-theme-dialog')

describe('DeleteThemeDialog', () => {
  beforeEach(() => {
    mockDeleteMutate.mockReset()
    mockReferences = undefined
  })

  it('blocks deletion when a theme is referenced', () => {
    mockReferences = {
      admin: true,
      status_pages: [{ id: 'status-1', name: 'Public Status' }]
    }

    render(<DeleteThemeDialog onClose={vi.fn()} theme={{ id: 7, name: 'In Use' }} />)

    expect(screen.getByText('appearance.custom_themes.delete_used_admin')).toBeInTheDocument()
    expect(screen.getByText('appearance.custom_themes.delete_used_status_page:Public Status')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'common:delete' })).not.toBeInTheDocument()
  })

  it('calls delete mutation when no references exist', () => {
    mockReferences = { admin: false, status_pages: [] }

    render(<DeleteThemeDialog onClose={vi.fn()} theme={{ id: 7, name: 'Unused' }} />)

    fireEvent.click(screen.getByRole('button', { name: 'common:delete' }))

    expect(mockDeleteMutate).toHaveBeenCalledWith(7, expect.any(Object))
  })
})
