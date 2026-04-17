import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusDot } from './status-dot'

describe('StatusDot', () => {
  it('renders pulsing emerald dot when online', () => {
    const { container } = render(<StatusDot online />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).toMatch(/animate-pulse/)
    expect(el?.className).toMatch(/bg-emerald-500/)
  })

  it('renders muted dot without pulse when offline', () => {
    const { container } = render(<StatusDot online={false} />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(/animate-pulse/)
    expect(el?.className).toMatch(/bg-muted-foreground/)
  })
})
