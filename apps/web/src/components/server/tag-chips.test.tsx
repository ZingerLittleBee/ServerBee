import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TagChips } from './tag-chips'

describe('TagChips', () => {
  it('renders nothing when tags is undefined', () => {
    const { container } = render(<TagChips tags={undefined} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing when tags is empty', () => {
    const { container } = render(<TagChips tags={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders each tag as a chip', () => {
    render(<TagChips tags={['CN2 GIA', 'AS9929', 'CMI']} />)
    expect(screen.getByText('CN2 GIA')).toBeDefined()
    expect(screen.getByText('AS9929')).toBeDefined()
    expect(screen.getByText('CMI')).toBeDefined()
  })
})
