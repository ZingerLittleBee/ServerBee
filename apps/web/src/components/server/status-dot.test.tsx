import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { deriveServerStatus, StatusDot } from './status-dot'

const ANIMATE_PULSE_RE = /animate-pulse/
const BG_EMERALD_RE = /bg-emerald-500/
const BG_MUTED_RE = /bg-muted-foreground/
const BG_AMBER_RE = /bg-amber-500/

describe('StatusDot', () => {
  it('renders pulsing emerald dot with online aria-label when status is online', () => {
    const { container } = render(<StatusDot status="online" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_EMERALD_RE)
    expect(el?.getAttribute('aria-label')).toBe('online')
  })

  it('renders muted dot without pulse when status is offline', () => {
    const { container } = render(<StatusDot status="offline" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_MUTED_RE)
    expect(el?.getAttribute('aria-label')).toBe('offline')
  })

  it('renders amber dot when status is pending', () => {
    const { container } = render(<StatusDot status="pending" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_AMBER_RE)
    expect(el?.getAttribute('aria-label')).toBe('pending')
  })
})

describe('deriveServerStatus', () => {
  it('returns online when online and has_token is true', () => {
    expect(deriveServerStatus({ online: true, has_token: true })).toBe('online')
  })

  it('returns offline when not online and has_token is true', () => {
    expect(deriveServerStatus({ online: false, has_token: true })).toBe('offline')
  })

  it('returns pending when has_token is false and offline', () => {
    expect(deriveServerStatus({ online: false, has_token: false })).toBe('pending')
  })

  it('returns pending when has_token is false even if online (has_token wins)', () => {
    expect(deriveServerStatus({ online: true, has_token: false })).toBe('pending')
  })

  it('treats undefined has_token as "has token, just old payload" (defensive default)', () => {
    expect(deriveServerStatus({ online: true })).toBe('online')
    expect(deriveServerStatus({ online: false })).toBe('offline')
  })
})
