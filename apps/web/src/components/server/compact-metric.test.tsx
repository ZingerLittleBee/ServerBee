import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CompactMetric } from './compact-metric'

describe('CompactMetric', () => {
  it('renders the label, value and optional sub-value', () => {
    render(<CompactMetric label="Read" subValue="peak" value="12.5" />)

    expect(screen.getByText('Read')).toBeInTheDocument()
    expect(screen.getByText('12.5')).toBeInTheDocument()
    expect(screen.getByText('peak')).toBeInTheDocument()
  })

  it('renders the value with tabular-nums so live updates do not shift the layout', () => {
    render(<CompactMetric label="Read" value="12.5" />)

    expect(screen.getByText('12.5')).toHaveClass('tabular-nums')
  })
})
