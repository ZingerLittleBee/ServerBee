import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockNavigate = vi.fn()
const mockUpdateMutate = vi.fn()

const theme = {
  based_on: 'default',
  created_at: '2026-04-30T00:00:00Z',
  description: null,
  id: 7,
  name: 'Original',
  updated_at: '2026-04-30T00:00:00Z',
  vars_dark: {
    background: 'oklch(0.1 0 0)',
    foreground: 'oklch(0.9 0 0)',
    primary: 'oklch(0.7 0.1 180)'
  },
  vars_light: {
    background: 'oklch(1 0 0)',
    foreground: 'oklch(0 0 0)',
    primary: 'oklch(0.5 0.1 180)'
  }
}

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/api/themes', () => ({
  useThemeQuery: () => ({
    data: theme,
    isLoading: false
  }),
  useUpdateTheme: () => ({
    isPending: false,
    mutate: mockUpdateMutate
  })
}))

const { ThemeEditor } = await import('./theme-editor')

describe('ThemeEditor', () => {
  beforeEach(() => {
    mockNavigate.mockReset()
    mockUpdateMutate.mockReset()
  })

  it('saves the edited theme name with both variable maps', () => {
    render(<ThemeEditor themeId={7} />)

    fireEvent.change(screen.getByDisplayValue('Original'), { target: { value: 'Renamed' } })
    fireEvent.click(screen.getByRole('button', { name: 'common:save' }))

    expect(mockUpdateMutate).toHaveBeenCalledWith(
      {
        body: {
          based_on: 'default',
          description: null,
          name: 'Renamed',
          vars_dark: theme.vars_dark,
          vars_light: theme.vars_light
        },
        id: 7
      },
      expect.any(Object)
    )
  })
})
