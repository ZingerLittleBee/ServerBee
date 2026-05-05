import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerCostOverview } from '@/lib/api-schema'
import { CostCell } from './cost-cell'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

function makeEntry(overrides: Partial<ServerCostOverview> = {}): ServerCostOverview {
  return {
    configured: true,
    name: 'srv',
    server_id: 'srv-1',
    ...overrides
  }
}

describe('CostCell', () => {
  it('renders not set when entry is missing', () => {
    render(<CostCell />)

    expect(screen.getByText('cost_not_set')).toBeDefined()
  })

  it('renders not set for missing price', () => {
    render(<CostCell entry={makeEntry({ configured: false, invalid_reason: 'missing_price' })} />)

    expect(screen.getByText('cost_not_set')).toBeDefined()
  })

  it('renders price only for missing billing cycle', () => {
    render(
      <CostCell
        entry={makeEntry({
          configured: false,
          currency: 'USD',
          invalid_reason: 'missing_billing_cycle'
        })}
      />
    )

    expect(screen.getByText('cost_price_only')).toBeDefined()
  })

  it('renders monthly equivalent cost with value score and grade', () => {
    render(
      <CostCell
        entry={makeEntry({
          cost_per_month_equivalent: 5,
          currency: 'USD',
          value_score: {
            confidence: 'high',
            grade: 'good',
            reasons: [],
            score: 82
          }
        })}
      />
    )

    expect(screen.getByText('cost_month_equivalent')).toBeDefined()
    expect(screen.getByText('cost_value_score')).toBeDefined()
    expect(screen.getByText('82')).toBeDefined()
    expect(screen.getByText('cost_grade_good')).toBeDefined()
  })

  it('renders daily cost when monthly equivalent is unavailable', () => {
    render(
      <CostCell
        entry={makeEntry({
          cost_per_day: 0.25,
          currency: 'USD'
        })}
      />
    )

    expect(screen.getByText('cost_per_day')).toBeDefined()
  })
})
