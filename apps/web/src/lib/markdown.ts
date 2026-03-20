const SAFE_SCHEMES = ['http:', 'https:', 'mailto:']

function escapeHtml(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function isSafeUrl(url: string): boolean {
  try {
    const parsed = new URL(url, 'https://placeholder.invalid')
    return SAFE_SCHEMES.includes(parsed.protocol)
  } catch {
    return false
  }
}

export function renderMarkdown(input: string): string {
  // First escape all raw HTML
  let text = escapeHtml(input)

  // Headings (### before ## to avoid partial match)
  text = text.replace(/^### (.+)$/gm, '<h3>$1</h3>')
  text = text.replace(/^## (.+)$/gm, '<h2>$1</h2>')

  // Bold and italic (bold before italic to avoid partial match)
  text = text.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
  text = text.replace(/\*(.+?)\*/g, '<em>$1</em>')

  // Inline code
  text = text.replace(/`([^`]+)`/g, '<code>$1</code>')

  // Links — only safe schemes
  text = text.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label: string, url: string) => {
    // Unescape the URL so we can parse it properly
    const rawUrl = url
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
    if (isSafeUrl(rawUrl)) {
      return `<a href="${url}" rel="noopener noreferrer" target="_blank">${label}</a>`
    }
    return label
  })

  // Unordered lists
  const lines = text.split('\n')
  const result: string[] = []
  let inList = false
  for (const line of lines) {
    if (line.startsWith('- ')) {
      if (!inList) {
        result.push('<ul>')
        inList = true
      }
      result.push(`<li>${line.slice(2)}</li>`)
    } else {
      if (inList) {
        result.push('</ul>')
        inList = false
      }
      if (line.trim() === '') {
        result.push('<br>')
      } else if (line.startsWith('<h')) {
        result.push(line)
      } else {
        result.push(`<p>${line}</p>`)
      }
    }
  }
  if (inList) {
    result.push('</ul>')
  }

  return result.join('\n')
}
