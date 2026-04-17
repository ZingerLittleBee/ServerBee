import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ScrollArea } from './scroll-area'

describe('ScrollArea', () => {
  it('renders a constrained viewport instead of a size-full viewport', () => {
    const { container } = render(
      <ScrollArea className="min-h-0 flex-1">
        <div>content</div>
      </ScrollArea>
    )

    expect(screen.getByText('content')).toBeInTheDocument()

    const root = container.querySelector('[data-slot="scroll-area"]')
    const viewport = container.querySelector('[data-slot="scroll-area-viewport"]')

    expect(root).toHaveClass('flex')
    expect(root).toHaveClass('min-h-0')
    expect(viewport).toHaveClass('flex-1')
    expect(viewport).toHaveClass('min-h-0')
    expect(viewport).not.toHaveClass('size-full')
  })
})
