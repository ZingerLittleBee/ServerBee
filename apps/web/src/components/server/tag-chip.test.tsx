import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TagChipRow } from './tag-chip'

describe('TagChipRow', () => {
  it('renders nothing when tags is empty', () => {
    const { container } = render(<TagChipRow tags={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing when tags is undefined', () => {
    const { container } = render(<TagChipRow tags={undefined} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders a chip per tag', () => {
    render(<TagChipRow tags={['prod', 'web']} />)
    expect(screen.getByText('prod')).toBeDefined()
    expect(screen.getByText('web')).toBeDefined()
  })

  it('assigns the same palette color to the same tag across renders', () => {
    const { container, rerender } = render(<TagChipRow tags={['prod']} />)
    const first = container.querySelector('[data-slot="tag-chip"]')?.className
    rerender(<TagChipRow tags={['prod']} />)
    const second = container.querySelector('[data-slot="tag-chip"]')?.className
    expect(first).toBe(second)
  })

  it('adds title attr on the chip element for tooltip / truncate fallback', () => {
    render(<TagChipRow tags={['long-tag-value']} />)
    const chip = screen.getByText('long-tag-value')
    expect(chip.getAttribute('title')).toBe('long-tag-value')
  })
})
