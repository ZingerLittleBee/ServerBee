import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockCreateMutate = vi.fn()
const mockNavigate = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => mockNavigate
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@/api/themes', () => ({
  useCreateTheme: () => ({
    isPending: false,
    mutate: mockCreateMutate
  })
}))

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  SelectValue: () => <span />
}))

const { NewThemePage } = await import('./themes.new')

describe('NewThemePage', () => {
  beforeEach(() => {
    mockCreateMutate.mockReset()
    mockNavigate.mockReset()
  })

  it('creates a theme forked from the default preset and navigates to the editor', () => {
    mockCreateMutate.mockImplementation((_body: unknown, options: { onSuccess: (theme: { id: number }) => void }) => {
      options.onSuccess({ id: 42 })
    })

    render(<NewThemePage />)

    fireEvent.change(screen.getByPlaceholderText('appearance.editor.name_placeholder'), {
      target: { value: 'My Theme' }
    })
    fireEvent.click(screen.getByRole('button', { name: 'common:create' }))

    expect(mockCreateMutate).toHaveBeenCalledWith(
      expect.objectContaining({
        based_on: 'default',
        name: 'My Theme',
        vars_dark: expect.any(Object),
        vars_light: expect.any(Object)
      }),
      expect.any(Object)
    )
    expect(mockNavigate).toHaveBeenCalledWith({
      params: { id: '42' },
      to: '/settings/appearance/themes/$id'
    })
  })
})
