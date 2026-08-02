import { act, fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { NetworkMetricValue } from './network-metric-value'
import type { ServerCardTooltipTarget } from './server-card-network-data'

const targets: ServerCardTooltipTarget[] = [{ latency: 106, lossRatio: 0, targetId: 't1', targetName: 'Tokyo' }]

function renderValue(withTargets: readonly ServerCardTooltipTarget[]) {
  return render(
    <TooltipProvider>
      <NetworkMetricValue targets={withTargets}>
        <span className="font-semibold">106ms</span>
      </NetworkMetricValue>
    </TooltipProvider>
  )
}

describe('NetworkMetricValue', () => {
  it('renders the value untouched when there is no breakdown to show', () => {
    const { container } = renderValue([])

    expect(screen.getByText('106ms')).toBeInTheDocument()
    expect(container.querySelector('button')).toBeNull()
  })

  it('makes the trigger focusable so the breakdown is reachable by keyboard', () => {
    renderValue(targets)

    const trigger = screen.getByRole('button')
    expect(trigger).toHaveTextContent('106ms')
    trigger.focus()
    expect(trigger).toHaveFocus()
  })

  it('shows the per-target breakdown on focus, not only on hover', async () => {
    renderValue(targets)

    const trigger = screen.getByRole('button')
    act(() => {
      trigger.focus()
      fireEvent.focus(trigger)
    })

    expect(await screen.findByText('Tokyo')).toBeInTheDocument()
  })
})
