import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CostInsightBar } from './cost-insight-bar'

const LOCAL_RENEWAL_DATE = /^Renewal deadline:.*2026-03-08/
const INDEPENDENT_PERIOD = /separate from the cost estimation period/

vi.mock('@/lib/api-client', () => ({ api: { get: vi.fn().mockResolvedValue(null) } }))

afterEach(cleanup)

describe('CostInsightBar renewal dates', () => {
  it('displays the server selected local date beside independent cost information', () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={client}>
        <CostInsightBar
          server={{
            billing_cycle: 'monthly',
            currency: 'USD',
            price: 12,
            expired_at: '2026-03-09T06:59:59.999999999Z',
            traffic_limit: null,
            traffic_limit_type: null,
            renewal: {
              enabled: false,
              billing_timezone: 'America/Los_Angeles',
              expiry_date: '2026-03-08',
              confirmed_expired_at: '2026-03-09T06:59:59.999999999Z',
              deadline_origin: 'confirmed',
              occurrence_id: null
            }
          }}
          serverId="calendar-server"
        />
      </QueryClientProvider>
    )
    expect(screen.getByText(LOCAL_RENEWAL_DATE)).toBeInTheDocument()
    expect(screen.getByText(INDEPENDENT_PERIOD)).toBeInTheDocument()
  })

  it('retains the frozen projected origin beside the current deadline and confirmed history', () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={client}>
        <CostInsightBar
          server={{
            billing_cycle: 'monthly',
            currency: 'USD',
            price: null,
            expired_at: '2026-03-09T03:59:59Z',
            traffic_limit: null,
            traffic_limit_type: null,
            renewal: {
              enabled: false,
              billing_timezone: 'America/New_York',
              expiry_date: '2026-03-08',
              confirmed_expired_at: '2026-02-09T04:59:59Z',
              deadline_origin: 'frozen',
              occurrence_id: 'opaque-1'
            }
          }}
          serverId="frozen-server"
        />
      </QueryClientProvider>
    )
    expect(screen.getByText('Frozen projected deadline')).toBeInTheDocument()
    expect(screen.getByText('Last operator-confirmed expiry: 2/8/2026 (America/New_York)')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Tracking advances the forecast after each expiry date ends. Disabling freezes the current deadline. This estimate does not confirm provider renewal or payment.'
      )
    ).toBeInTheDocument()
  })
})
