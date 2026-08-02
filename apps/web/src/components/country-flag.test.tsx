import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { CountryFlag } from './country-flag'

function renderFlag(code: string | null | undefined) {
  return render(
    <TooltipProvider>
      <CountryFlag code={code} />
    </TooltipProvider>
  )
}

describe('CountryFlag', () => {
  it('renders the flag emoji for a valid code', () => {
    const { container } = renderFlag('jp')
    expect(container.textContent).toContain('🇯🇵')
  })

  it('renders nothing for a missing or invalid code', () => {
    const { container } = renderFlag(null)
    expect(container.innerHTML).toBe('')

    const { container: invalid } = renderFlag('XYZ')
    expect(invalid.innerHTML).toBe('')
  })

  it('exposes the country name to assistive tech while hiding the decorative emoji', () => {
    renderFlag('jp')

    expect(screen.getByText('Japan')).toHaveClass('sr-only')
    expect(screen.getByText('🇯🇵')).toHaveAttribute('aria-hidden', 'true')
  })

  it('does not add a tab stop for the decorative emoji', () => {
    const { container } = renderFlag('jp')
    expect(container.querySelectorAll('[tabindex], button, a')).toHaveLength(0)
  })

  it('does not duplicate the tooltip with a native title attribute', () => {
    const { container } = renderFlag('jp')
    expect(container.querySelector('[title]')).toBeNull()
  })
})
