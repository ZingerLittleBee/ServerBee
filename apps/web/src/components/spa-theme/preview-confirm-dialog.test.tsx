import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import '@/lib/i18n'
import { PreviewConfirmDialog } from './preview-confirm-dialog'

const OPEN_PREVIEW_RE = /Open preview/

describe('PreviewConfirmDialog', () => {
  it('calls onConfirm when the confirm button is clicked', () => {
    const onConfirm = vi.fn()
    render(<PreviewConfirmDialog onConfirm={onConfirm} onOpenChange={vi.fn()} open />)
    fireEvent.click(screen.getByRole('button', { name: OPEN_PREVIEW_RE }))
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })
})
