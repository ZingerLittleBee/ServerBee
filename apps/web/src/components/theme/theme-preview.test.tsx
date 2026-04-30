import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ThemePreview } from './theme-preview'

describe('ThemePreview', () => {
  it('applies theme variables to the preview root', () => {
    render(<ThemePreview dark={false} vars={{ background: 'oklch(1 0 0)', foreground: 'oklch(0 0 0)' }} />)

    const preview = screen.getByTestId('theme-preview')

    expect(preview).toHaveStyle('--background: oklch(1 0 0)')
    expect(preview).toHaveStyle('--foreground: oklch(0 0 0)')
    expect(preview).not.toHaveClass('dark')
  })

  it('adds the dark class for dark preview mode', () => {
    render(<ThemePreview dark vars={{ background: 'oklch(0 0 0)' }} />)

    expect(screen.getByTestId('theme-preview')).toHaveClass('dark')
  })
})
