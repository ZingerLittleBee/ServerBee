import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ThemeCard } from './theme-card'

const TOKYO_NIGHT_RE = /Tokyo Night/

describe('ThemeCard', () => {
  it('activates the theme from the main card button', () => {
    const onActivate = vi.fn()

    render(<ThemeCard active={false} name="Tokyo Night" onActivate={onActivate} preview={['#111111', '#222222']} />)

    fireEvent.click(screen.getByRole('button', { name: TOKYO_NIGHT_RE }))

    expect(onActivate).toHaveBeenCalledTimes(1)
  })

  it('renders edit, duplicate, and delete actions without activating the theme', () => {
    const onActivate = vi.fn()
    const onEdit = vi.fn()
    const onDuplicate = vi.fn()
    const onDelete = vi.fn()

    render(
      <ThemeCard
        actions={{ onDelete, onDuplicate, onEdit }}
        active
        name="Custom Theme"
        onActivate={onActivate}
        preview={['#111111']}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'appearance.custom_themes.edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'appearance.custom_themes.duplicate' }))
    fireEvent.click(screen.getByRole('button', { name: 'appearance.custom_themes.delete' }))

    expect(onEdit).toHaveBeenCalledTimes(1)
    expect(onDuplicate).toHaveBeenCalledTimes(1)
    expect(onDelete).toHaveBeenCalledTimes(1)
    expect(onActivate).not.toHaveBeenCalled()
  })
})
