/* Shared helpers for the landing view models. Everything here is pure and deterministic: the same
   inputs give the same output in every JavaScript engine, so the server render (tick 0) always
   matches the first client render. */

import type { CSSProperties } from 'react'

declare module 'react' {
  interface CSSProperties {
    /** Fill color of a server-card dial or dashboard gauge. */
    '--c'?: string
    /** Fill level (0-100) of a server-card dial, dashboard gauge or iOS risk ring. */
    '--v'?: string
  }
}

const MODULUS = 2_147_483_647
const MULTIPLIER = 16_807
const LIMB = 65_536

/** One Park-Miller step. The product stays below 2^46, so it is exact in a double. */
function lehmer(x: number): number {
  return (x * MULTIPLIER) % MODULUS
}

/** (x * x) % MODULUS, splitting one factor into 16-bit limbs so every product stays below 2^48. */
function square(x: number): number {
  const high = Math.floor(x / LIMB)
  const low = x % LIMB
  return (((x * high) % MODULUS) * LIMB + x * low) % MODULUS
}

/**
 * Deterministic noise in [0, 1) for integer inputs; `stream` keeps different uses of the same
 * `(a, b)` pair independent. Park-Miller steps spread the input and the modular squaring makes the
 * result nonlinear, so neighbouring inputs are uncorrelated. Only +, -, *, / and % are used, which
 * IEEE 754 defines exactly (unlike Math.sin), so every engine returns the same value.
 */
export function noise(stream: number, a: number, b: number): number {
  const seed = ((stream * 7919 + a) * 104_729 + b) % (MODULUS - 1)
  let x = (seed < 0 ? seed + MODULUS - 1 : seed) + 1
  for (let round = 0; round < 2; round += 1) {
    x = square(lehmer(x))
  }
  x = lehmer(x)
  return (x - 1) / (MODULUS - 1)
}

const PI = Math.PI
const TAU = 2 * PI

/** A smooth wave shaped like Math.sin (Bhaskara I approximation), built from exact arithmetic only. */
function wave(x: number): number {
  const phase = ((x % TAU) + TAU) % TAU
  const half = phase < PI ? phase : phase - PI
  const product = half * (PI - half)
  const value = (16 * product) / (5 * PI * PI - 4 * product)
  return phase < PI ? value : -value
}

export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value))
}

/** One decimal place, like the product's metric values. */
export function f1(value: number): string {
  return (Math.round(value * 10) / 10).toFixed(1)
}

export function mean(values: readonly number[]): number {
  let sum = 0
  for (const value of values) {
    sum += value
  }
  return sum / values.length
}

/** Uniform jitter in [-amp, amp] that changes every tick. */
export function jitter(seed: number, tick: number, amp: number): number {
  return (noise(1, seed, tick) - 0.5) * 2 * amp
}

/** One noisy series window [start, start + count). Spikes are single sharp peaks. */
export function series(
  seed: number,
  base: number,
  amp: number,
  count: number,
  start: number,
  spikeEvery = 0,
  spike = 0
): number[] {
  const values: number[] = []
  for (let i = 0; i < count; i += 1) {
    const k = start + i
    let value = base + (noise(2, seed, k) - 0.5) * 2 * amp + wave((k + seed) / 5) * amp * 0.5
    if (spikeEvery > 0) {
      const phase = (k + seed * 3) % spikeEvery
      if (phase === 0) {
        value += spike
      } else if (phase === 1 || phase === spikeEvery - 1) {
        value += spike * 0.18
      }
    }
    values.push(Math.max(0, value))
  }
  return values
}

/** SVG polyline points in a 0..100 viewBox (y grows downward). */
export function pts(values: readonly number[], max: number): string {
  const last = values.length - 1
  return values
    .map((value, i) => `${((i / last) * 100).toFixed(2)},${(100 - clamp(value / max, 0, 1) * 100).toFixed(2)}`)
    .join(' ')
}

/** One segment of an iOS segmented control. */
export interface SegItem {
  /** `on` for the selected segment, otherwise undefined. */
  className?: string
  id: string
  label: string
}

/** One chart series: an SVG polyline colored through `style.color`. */
export interface ChartLine {
  id: string
  pts: string
  style: CSSProperties
}

export interface CommandSegment {
  /**
   * What follows the segment in the command: the space after a word, or '' where the segment ends
   * inside a word (a URL or path part) or ends the command.
   */
  gap: string
  /** Offset of the segment in the command, unique within it. */
  id: string
  text: string
}

/** Flag-value pairs and quoted strings up to this length stay on one line; longer ones keep their break points. */
const MAX_UNBROKEN = 24

/** Shell operators, never the value of the flag before them. */
const operators = new Set(['|', '||', '&&', ';', '>', '>>'])

/** A word of the command. A whole word is one segment; the others split into URL and path parts. */
interface Word {
  text: string
  whole: boolean
}

/** Joins the words of a quoted string ('"disk check"') into one whole word when it is at most MAX_UNBROKEN long. */
function quotedWords(words: readonly string[]): Word[] {
  const result: Word[] = []
  for (let i = 0; i < words.length; i += 1) {
    const word = words[i]
    const quote = word.charAt(0)
    if (quote === '"' || quote === "'") {
      let end = i
      while (end < words.length && !(words[end].endsWith(quote) && (end > i || word.length > 1))) {
        end += 1
      }
      const quoted = words.slice(i, end + 1).join(' ')
      if (end < words.length && quoted.length <= MAX_UNBROKEN) {
        result.push({ text: quoted, whole: true })
        i = end
        continue
      }
    }
    result.push({ text: word, whole: false })
  }
  return result
}

/** Keeps a flag with its value ('--method docker', '--for 30m') when the pair is at most MAX_UNBROKEN long. */
function flagPairs(words: readonly Word[]): Word[] {
  const result: Word[] = []
  for (let i = 0; i < words.length; i += 1) {
    const word = words[i]
    const value = words[i + 1]
    const isFlag = word.text.startsWith('-')
    const isValue =
      value !== undefined && value.text !== '' && !value.text.startsWith('-') && !operators.has(value.text)
    if (isFlag && isValue && word.text.length + 1 + value.text.length <= MAX_UNBROKEN) {
      result.push({ text: `${word.text} ${value.text}`, whole: true })
      i += 1
    } else {
      result.push(word)
    }
  }
  return result
}

/** A one-letter flag such as '-y'. */
const shortFlag = /^-[A-Za-z]$/

/** A short flag that ends the command joins the unbroken word before it ('--method docker -y'), so it never sits alone. */
function trailingFlag(words: readonly Word[]): readonly Word[] {
  const last = words.at(-1)
  const previous = words.at(-2)
  if (!(last && previous?.whole && shortFlag.test(last.text))) {
    return words
  }
  const text = `${previous.text} ${last.text}`
  return text.length > MAX_UNBROKEN ? words : [...words.slice(0, -2), { text, whole: true }]
}

/**
 * Splits a word after each '/' of a path and, in a bare URL, after its '://': 'https://', 'raw.githubusercontent.com/'.
 * A quoted URL ('https://panel.example.com') keeps its scheme and host together.
 */
function wordParts(word: string): string[] {
  const quoted = word.startsWith('"') || word.startsWith("'")
  const parts: string[] = []
  let part = ''
  for (let i = 0; i < word.length; i += 1) {
    const char = word.charAt(i)
    const prev = word.charAt(i - 1)
    const next = word.charAt(i + 1)
    part += char
    const pathSlash = prev !== '/' && prev !== ':' && next !== '/'
    const schemeEnd = !quoted && prev === '/' && word.charAt(i - 2) === ':'
    if (char === '/' && next !== '' && (pathSlash || schemeEnd)) {
      parts.push(part)
      part = ''
    }
  }
  parts.push(part)
  return parts
}

/**
 * Splits a shell command into unbreakable segments. Lines break only at the spaces between words and
 * after URL and path parts, never inside a flag such as --method, between a flag and its short value,
 * or inside a short quoted string. The segments and their gaps add up to the exact command, so copying
 * the rendered text yields the command.
 */
export function segs(command: string): CommandSegment[] {
  const words = trailingFlag(flagPairs(quotedWords(command.split(' '))))
  const segments: CommandSegment[] = []
  let offset = 0
  for (const [wordIndex, word] of words.entries()) {
    const parts = word.whole ? [word.text] : wordParts(word.text)
    for (const [partIndex, text] of parts.entries()) {
      const gap = partIndex === parts.length - 1 && wordIndex < words.length - 1 ? ' ' : ''
      segments.push({ id: String(offset), text, gap })
      offset += text.length + gap.length
    }
  }
  return segments
}

/** Status tone of a latency or loss value, used as a class name (`cell ok`, `sv-val t-warn`). */
type Tone = 'ok' | 'warn' | 'sev' | 'fail' | 'none'

export function latencyTone(ms: number | null): Tone {
  if (ms === null) {
    return 'none'
  }
  return ms >= 300 ? 'warn' : 'ok'
}

export function lossTone(percent: number | null): Tone {
  if (percent === null) {
    return 'none'
  }
  if (percent >= 100) {
    return 'fail'
  }
  if (percent >= 5) {
    return 'sev'
  }
  return percent >= 1 ? 'warn' : 'ok'
}

/** Dial and gauge color by usage, as in the product. */
export function usageColor(value: number): string {
  if (value > 90) {
    return 'var(--p-red)'
  }
  return value > 70 ? 'var(--p-amber)' : 'var(--p-green)'
}
