import { converter, formatHex, formatHex8, parse } from 'culori'

export interface OklchValue {
  alpha?: number
  alphaIsPercent?: boolean
  c: number
  h: number
  l: number
}

const toOklch = converter('oklch')

const OKLCH_RE =
  /^oklch\(\s*((?:\d+(?:\.\d+)?)|(?:\.\d+))\s+((?:\d+(?:\.\d+)?)|(?:\.\d+))\s+((?:\d+(?:\.\d+)?)|(?:\.\d+))(?:\s*\/\s*((?:\d+(?:\.\d+)?)|(?:\.\d+))(%?)?)?\s*\)$/

function roundComponent(value: number): string {
  const rounded = Number(value.toFixed(4))

  return Object.is(rounded, -0) ? '0' : String(rounded)
}

export function parseOklch(s: string): OklchValue | null {
  const match = OKLCH_RE.exec(s.trim())

  if (!match) {
    return null
  }

  const [, l, c, h, alpha, alphaPercent] = match
  const value: OklchValue = {
    c: Number(c),
    h: Number(h),
    l: Number(l)
  }

  if (alpha !== undefined) {
    value.alpha = Number(alpha)
    value.alphaIsPercent = alphaPercent === '%'
  }

  return value
}

export function formatOklch(v: OklchValue): string {
  const components = `oklch(${roundComponent(v.l)} ${roundComponent(v.c)} ${roundComponent(v.h)}`

  if (v.alpha === undefined) {
    return `${components})`
  }

  const alpha = `${roundComponent(v.alpha)}${v.alphaIsPercent ? '%' : ''}`

  return `${components} / ${alpha})`
}

export function oklchToHex(s: string): string | null {
  const color = parse(s)

  if (!color) {
    return null
  }

  const hex = color.alpha === undefined || color.alpha >= 1 ? formatHex(color) : formatHex8(color)

  return hex?.toLowerCase() ?? null
}

export function hexToOklch(hex: string): string | null {
  const color = parse(hex)

  if (!color) {
    return null
  }

  const oklch = toOklch(color)

  if (!oklch) {
    return null
  }

  return formatOklch({
    alpha: oklch.alpha,
    c: oklch.c,
    h: oklch.h ?? 0,
    l: oklch.l
  })
}
