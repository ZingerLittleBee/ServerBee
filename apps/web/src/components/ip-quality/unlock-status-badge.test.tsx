import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { UnlockStatus } from '@/lib/ip-quality-types'
import { UnlockStatusBadge } from './unlock-status-badge'

const cases: { status: UnlockStatus; label: string; toneFragment: string }[] = [
  { status: 'unlocked', label: 'Unlocked', toneFragment: 'green' },
  { status: 'restricted', label: 'Restricted', toneFragment: 'amber' },
  { status: 'blocked', label: 'Blocked', toneFragment: 'red' },
  { status: 'failed', label: 'Failed', toneFragment: 'muted-foreground' },
  { status: 'unsupported', label: 'Unsupported', toneFragment: 'muted' }
]

describe('UnlockStatusBadge', () => {
  for (const c of cases) {
    it(`renders ${c.status} with the right label`, () => {
      render(<UnlockStatusBadge status={c.status} />)
      expect(screen.getByText(c.label)).toBeInTheDocument()
    })

    it(`renders ${c.status} with a ${c.toneFragment} tone`, () => {
      const { container } = render(<UnlockStatusBadge status={c.status} />)
      const badge = container.querySelector('[data-slot="badge"]')
      expect(badge?.className).toContain(c.toneFragment)
    })
  }

  it('falls back to a muted tone for an unknown status', () => {
    const { container } = render(<UnlockStatusBadge status={'weird' as UnlockStatus} />)
    const badge = container.querySelector('[data-slot="badge"]')
    expect(badge?.className).toContain('muted')
  })
})
