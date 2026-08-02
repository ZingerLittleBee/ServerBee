import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MetricValue } from './metric-value'

describe('MetricValue', () => {
  it('splits the formatted value from its unit in the dense variant', () => {
    render(<MetricValue kind="bytes" value={1536} />)

    expect(screen.getByText('1.5')).toBeInTheDocument()
    expect(screen.getByText('KB')).toBeInTheDocument()
  })

  it('collapses non-positive speeds to a plain zero', () => {
    render(<MetricValue kind="speed" value={0} />)

    expect(screen.getByText('0')).toBeInTheDocument()
  })

  it('renders the unit as a subdued suffix in the compact variant', () => {
    render(<MetricValue kind="speed" value={2048} variant="compact" />)

    expect(screen.getByText('2.0')).toBeInTheDocument()
    expect(screen.getByText('KB/s')).toHaveClass('text-muted-foreground')
  })

  it('carries tabular-nums itself so digits do not jitter as values tick', () => {
    const { rerender } = render(<MetricValue kind="bytes" value={1536} />)
    expect(screen.getByText('1.5')).toHaveClass('tabular-nums')

    rerender(<MetricValue kind="speed" value={2048} variant="compact" />)
    expect(screen.getByText('2.0')).toHaveClass('tabular-nums')

    rerender(<MetricValue kind="speed" value={0} />)
    expect(screen.getByText('0')).toHaveClass('tabular-nums')

    rerender(<MetricValue kind="speed" value={0} variant="compact" />)
    expect(screen.getByText('0')).toHaveClass('tabular-nums')
  })
})
