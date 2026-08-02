import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusDot } from './status-dot'
import { deriveServerStatus } from './status-dot-utils'

const ANIMATE_PULSE_RE = /animate-pulse/
const BG_HEALTHY_RE = /bg-status-healthy/
const BG_MUTED_RE = /bg-muted-foreground/
const BG_WARNING_RE = /bg-status-warning/

describe('StatusDot', () => {
  it('renders pulsing healthy dot with online aria-label when status is online', () => {
    const { container } = render(<StatusDot status="online" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_HEALTHY_RE)
    expect(el?.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByText('Online')).toHaveClass('sr-only')
  })

  it('renders muted dot without pulse when status is offline', () => {
    const { container } = render(<StatusDot status="offline" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_MUTED_RE)
    expect(el?.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByText('Offline')).toHaveClass('sr-only')
  })

  it('renders warning-toned dot when status is pending', () => {
    const { container } = render(<StatusDot status="pending" />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(ANIMATE_PULSE_RE)
    expect(el?.className).toMatch(BG_WARNING_RE)
    expect(el?.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByText('Pending')).toHaveClass('sr-only')
  })
})

describe('deriveServerStatus', () => {
  it('returns online when authority is claimed and the agent is online', () => {
    expect(deriveServerStatus({ agent_authority: { outstanding_offer: null, status: 'claimed' }, online: true })).toBe(
      'online'
    )
  })

  it('returns offline when authority is claimed and the agent is offline', () => {
    expect(deriveServerStatus({ agent_authority: { outstanding_offer: null, status: 'claimed' }, online: false })).toBe(
      'offline'
    )
  })

  it('returns pending when authority is unclaimed', () => {
    expect(
      deriveServerStatus({ agent_authority: { outstanding_offer: null, status: 'unclaimed' }, online: false })
    ).toBe('pending')
  })

  it('lets unclaimed authority win over a stale online fact', () => {
    expect(
      deriveServerStatus({ agent_authority: { outstanding_offer: null, status: 'unclaimed' }, online: true })
    ).toBe('pending')
  })

  it('falls back to has_token only for legacy payloads', () => {
    expect(deriveServerStatus({ has_token: false, online: false })).toBe('pending')
    expect(deriveServerStatus({ has_token: true, online: true })).toBe('online')
  })
})
