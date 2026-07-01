import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MarkdownContent } from './markdown-content'

describe('MarkdownContent', () => {
  it('renders markdown without creating unsafe HTML sinks', () => {
    const { container } = render(
      <MarkdownContent
        content={'## Title\n[Docs](https://example.com)\n[xss](javascript:alert(1))\n<script>alert(1)</script>'}
      />
    )

    expect(screen.getByRole('heading', { level: 2, name: 'Title' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Docs' })).toHaveAttribute('href', 'https://example.com')
    expect(screen.queryByRole('link', { name: 'xss' })).not.toBeInTheDocument()
    expect(screen.getByText('xss')).toBeInTheDocument()
    expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument()
    expect(container.querySelector('script')).toBeNull()
  })
})
