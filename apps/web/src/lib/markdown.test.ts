import { describe, expect, it } from 'vitest'
import { renderMarkdown } from './markdown'

describe('renderMarkdown', () => {
  it('renders headings', () => {
    expect(renderMarkdown('## Title')).toContain('<h2>Title</h2>')
    expect(renderMarkdown('### Sub')).toContain('<h3>Sub</h3>')
  })

  it('renders bold and italic', () => {
    expect(renderMarkdown('**bold**')).toContain('<strong>bold</strong>')
    expect(renderMarkdown('*italic*')).toContain('<em>italic</em>')
  })

  it('renders safe links', () => {
    const result = renderMarkdown('[Google](https://google.com)')
    expect(result).toContain('href="https://google.com"')
    expect(result).toContain('rel="noopener noreferrer"')
  })

  it('blocks javascript: links', () => {
    const result = renderMarkdown('[xss](javascript:alert(1))')
    expect(result).not.toContain('href')
    expect(result).not.toContain('javascript')
  })

  it('escapes raw HTML tags', () => {
    const result = renderMarkdown('<script>alert(1)</script>')
    expect(result).not.toContain('<script>')
    expect(result).toContain('&lt;script&gt;')
  })

  it('escapes img onerror', () => {
    const result = renderMarkdown('<img onerror="alert(1)">')
    expect(result).not.toContain('<img')
    expect(result).toContain('&lt;img')
  })

  it('renders inline code', () => {
    expect(renderMarkdown('use `npm install`')).toContain('<code>npm install</code>')
  })

  it('renders unordered lists', () => {
    const result = renderMarkdown('- item 1\n- item 2')
    expect(result).toContain('<li>item 1</li>')
    expect(result).toContain('<li>item 2</li>')
  })
})
