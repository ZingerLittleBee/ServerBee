import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DateTicker } from './date-ticker'

describe('DateTicker', () => {
  it('does not render an empty pill when the current label is suppressed', () => {
    const { container } = render(<DateTicker currentIndex={1} labels={['00:54', '']} visible />)

    expect(container).toBeEmptyDOMElement()
  })

  it('renders the current non-empty label', () => {
    render(<DateTicker currentIndex={1} labels={['00:54', '00:55']} visible />)

    expect(screen.getByText('00:55')).toBeInTheDocument()
  })
})
