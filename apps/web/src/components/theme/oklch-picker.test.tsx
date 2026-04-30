import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { OklchPicker } from './oklch-picker'

const OKLCH_PREFIX_RE = /^oklch\(/

describe('OklchPicker', () => {
  it('updates the L channel through the slider', () => {
    const onChange = vi.fn()

    render(<OklchPicker onChange={onChange} value="oklch(0.5 0.1 180)" />)

    fireEvent.change(screen.getByLabelText('L'), { target: { value: '0.75' } })

    expect(onChange).toHaveBeenCalledWith('oklch(0.75 0.1 180)')
  })

  it('converts valid hex input to OKLCH', () => {
    const onChange = vi.fn()

    render(<OklchPicker onChange={onChange} value="oklch(0.5 0.1 180)" />)

    fireEvent.change(screen.getByPlaceholderText('#rrggbb'), { target: { value: '#ff0000' } })

    expect(onChange).toHaveBeenCalledWith(expect.stringMatching(OKLCH_PREFIX_RE))
  })
})
