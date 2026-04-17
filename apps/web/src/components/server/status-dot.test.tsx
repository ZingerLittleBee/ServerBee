import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusDot } from './status-dot'

const ANIMATE_PULSE_RE = /animate-pulse/
const BG_EMERALD_RE = /bg-emerald-500/
const BG_MUTED_RE = /bg-muted-foreground/

describe('StatusDot', () => {
  it('renders pulsing emerald dot when online', () => {
    const { container } = render(<StatusDot online />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_EMERALD_RE)
  })

  it('renders muted dot without pulse when offline', () => {
    const { container } = render(<StatusDot online={false} />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_MUTED_RE)
  })
})
