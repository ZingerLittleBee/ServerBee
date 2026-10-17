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

  it('renders level-1 headings (# ) instead of leaving raw markdown', () => {
    render(<MarkdownContent content={'# Heading One\nbody'} />)

    expect(screen.getByRole('heading', { level: 1, name: 'Heading One' })).toBeInTheDocument()
    expect(screen.queryByText('# Heading One')).not.toBeInTheDocument()
  })

  it('renders level-2 and level-3 headings', () => {
    render(<MarkdownContent content={'## Title\n### Subtitle'} />)

    expect(screen.getByRole('heading', { level: 2, name: 'Title' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 3, name: 'Subtitle' })).toBeInTheDocument()
  })

  it('renders bold and italic text', () => {
    const { container } = render(<MarkdownContent content={'**bold** and *italic*'} />)

    expect(container.querySelector('strong')).toHaveTextContent('bold')
    expect(container.querySelector('em')).toHaveTextContent('italic')
  })

  it('renders safe links with isolated browsing context', () => {
    render(<MarkdownContent content={'[Docs](https://example.com)'} />)

    expect(screen.getByRole('link', { name: 'Docs' })).toHaveAttribute('href', 'https://example.com')
    expect(screen.getByRole('link', { name: 'Docs' })).toHaveAttribute('rel', 'noopener noreferrer')
    expect(screen.getByRole('link', { name: 'Docs' })).toHaveAttribute('target', '_blank')
  })

  it.each(['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>'])('blocks unsafe links: %s', (url) => {
    const { container } = render(<MarkdownContent content={`[unsafe](${url})`} />)

    expect(screen.getByText('unsafe')).toBeInTheDocument()
    expect(screen.queryByRole('link')).not.toBeInTheDocument()
    expect(container).not.toHaveTextContent(url)
  })

  it('renders raw image event handlers as text', () => {
    const source = '<img onerror="alert(1)">'
    const { container } = render(<MarkdownContent content={source} />)

    expect(screen.getByText(source)).toBeInTheDocument()
    expect(container.querySelector('img')).toBeNull()
    expect(container.querySelector('[onerror]')).toBeNull()
  })

  it('renders inline code literally', () => {
    const { container } = render(<MarkdownContent content={'use `**npm install**`'} />)

    expect(container.querySelector('code')).toHaveTextContent('**npm install**')
    expect(container.querySelector('strong')).toBeNull()
  })

  it('renders unordered list items and closes the list before a paragraph', () => {
    render(<MarkdownContent content={'- item 1\n- item 2\nbody'} />)

    const list = screen.getByRole('list')
    expect(screen.getAllByRole('listitem').map((item) => item.textContent)).toEqual(['item 1', 'item 2'])
    expect(list).not.toContainElement(screen.getByText('body'))
  })
})
