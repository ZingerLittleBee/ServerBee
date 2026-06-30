import { Fragment, useMemo } from 'react'

const SAFE_SCHEMES = ['http:', 'https:', 'mailto:']

type InlineToken =
  | { kind: 'code'; key: string; text: string }
  | { children: InlineToken[]; kind: 'em'; key: string }
  | { children: InlineToken[]; kind: 'link'; key: string; url: string }
  | { children: InlineToken[]; kind: 'plainLinkLabel'; key: string }
  | { children: InlineToken[]; kind: 'strong'; key: string }
  | { kind: 'text'; key: string; text: string }

type MarkdownBlock =
  | { content: InlineToken[]; kind: 'h2'; key: string }
  | { content: InlineToken[]; kind: 'h3'; key: string }
  | { content: InlineToken[]; kind: 'paragraph'; key: string }
  | { items: { content: InlineToken[]; key: string }[]; kind: 'list'; key: string }
  | { kind: 'break'; key: string }

function isSafeUrl(url: string): boolean {
  try {
    const parsed = new URL(url, 'https://placeholder.invalid')
    return SAFE_SCHEMES.includes(parsed.protocol)
  } catch {
    return false
  }
}

function findClosingParen(input: string, openParenIndex: number): number {
  let depth = 1
  for (let i = openParenIndex + 1; i < input.length; i += 1) {
    if (input[i] === '(') {
      depth += 1
    } else if (input[i] === ')') {
      depth -= 1
      if (depth === 0) {
        return i
      }
    }
  }
  return -1
}

function readLinkToken(input: string, index: number, key: string): { end: number; token: InlineToken } | null {
  const labelEnd = input.indexOf(']', index + 1)
  if (labelEnd === -1 || input[labelEnd + 1] !== '(') {
    return null
  }

  const urlEnd = findClosingParen(input, labelEnd + 1)
  if (urlEnd === -1) {
    return null
  }

  const label = input.slice(index + 1, labelEnd)
  const url = input.slice(labelEnd + 2, urlEnd)
  const children = parseInlineTokens(label, key)
  if (isSafeUrl(url)) {
    return { end: urlEnd + 1, token: { children, kind: 'link', key, url } }
  }
  return { end: urlEnd + 1, token: { children, kind: 'plainLinkLabel', key } }
}

function readDelimitedToken(
  input: string,
  index: number,
  delimiter: '`' | '*' | '**',
  key: string
): { end: number; token: InlineToken } | null {
  const end = input.indexOf(delimiter, index + delimiter.length)
  if (end === -1) {
    return null
  }

  const inner = input.slice(index + delimiter.length, end)
  if (delimiter === '`') {
    return { end: end + delimiter.length, token: { kind: 'code', key, text: inner } }
  }
  if (delimiter === '**') {
    return { end: end + delimiter.length, token: { children: parseInlineTokens(inner, key), kind: 'strong', key } }
  }
  return { end: end + delimiter.length, token: { children: parseInlineTokens(inner, key), kind: 'em', key } }
}

function readInlineToken(input: string, index: number, key: string): { end: number; token: InlineToken } | null {
  if (input.startsWith('**', index)) {
    return readDelimitedToken(input, index, '**', key)
  }
  if (input[index] === '`') {
    return readDelimitedToken(input, index, '`', key)
  }
  if (input[index] === '*') {
    return readDelimitedToken(input, index, '*', key)
  }
  if (input[index] === '[') {
    return readLinkToken(input, index, key)
  }
  return null
}

function parseInlineTokens(input: string, keyPrefix: string): InlineToken[] {
  const tokens: InlineToken[] = []
  let cursor = 0
  let textStart = 0
  let tokenIndex = 0

  const pushText = (end: number) => {
    if (end > textStart) {
      tokens.push({ kind: 'text', key: `${keyPrefix}-text-${tokenIndex}`, text: input.slice(textStart, end) })
      tokenIndex += 1
    }
  }

  while (cursor < input.length) {
    const token = readInlineToken(input, cursor, `${keyPrefix}-token-${tokenIndex}`)
    if (!token) {
      cursor += 1
      continue
    }
    pushText(cursor)
    tokens.push(token.token)
    tokenIndex += 1
    cursor = token.end
    textStart = token.end
  }

  pushText(input.length)
  return tokens
}

function parseMarkdownBlocks(content: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = []
  let listItems: { content: InlineToken[]; key: string }[] = []

  const flushList = () => {
    if (listItems.length === 0) {
      return
    }
    blocks.push({ items: listItems, key: `block-${blocks.length}`, kind: 'list' })
    listItems = []
  }

  for (const [lineIndex, line] of content.split('\n').entries()) {
    if (line.startsWith('- ')) {
      const key = `line-${lineIndex}`
      listItems.push({ content: parseInlineTokens(line.slice(2), key), key })
      continue
    }

    flushList()
    const key = `block-${blocks.length}`
    if (line.trim() === '') {
      blocks.push({ key, kind: 'break' })
    } else if (line.startsWith('### ')) {
      blocks.push({ content: parseInlineTokens(line.slice(4), key), key, kind: 'h3' })
    } else if (line.startsWith('## ')) {
      blocks.push({ content: parseInlineTokens(line.slice(3), key), key, kind: 'h2' })
    } else {
      blocks.push({ content: parseInlineTokens(line, key), key, kind: 'paragraph' })
    }
  }

  flushList()
  return blocks
}

function InlineTokenView({ token }: { token: InlineToken }) {
  switch (token.kind) {
    case 'code':
      return <code>{token.text}</code>
    case 'em':
      return (
        <em>
          <InlineTokens tokens={token.children} />
        </em>
      )
    case 'link':
      return (
        <a href={token.url} rel="noopener noreferrer" target="_blank">
          <InlineTokens tokens={token.children} />
        </a>
      )
    case 'plainLinkLabel':
      return <InlineTokens tokens={token.children} />
    case 'strong':
      return (
        <strong>
          <InlineTokens tokens={token.children} />
        </strong>
      )
    case 'text':
      return <>{token.text}</>
    default:
      return null
  }
}

function InlineTokens({ tokens }: { tokens: InlineToken[] }) {
  return (
    <>
      {tokens.map((token) => (
        <Fragment key={token.key}>
          <InlineTokenView token={token} />
        </Fragment>
      ))}
    </>
  )
}

function MarkdownBlockView({ block }: { block: MarkdownBlock }) {
  switch (block.kind) {
    case 'break':
      return <br />
    case 'h2':
      return (
        <h2>
          <InlineTokens tokens={block.content} />
        </h2>
      )
    case 'h3':
      return (
        <h3>
          <InlineTokens tokens={block.content} />
        </h3>
      )
    case 'list':
      return (
        <ul>
          {block.items.map((item) => (
            <li key={item.key}>
              <InlineTokens tokens={item.content} />
            </li>
          ))}
        </ul>
      )
    case 'paragraph':
      return (
        <p>
          <InlineTokens tokens={block.content} />
        </p>
      )
    default:
      return null
  }
}

export function MarkdownContent({ className, content }: { className?: string; content: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(content), [content])

  return (
    <div className={className}>
      {blocks.map((block) => (
        <Fragment key={block.key}>
          <MarkdownBlockView block={block} />
        </Fragment>
      ))}
    </div>
  )
}
