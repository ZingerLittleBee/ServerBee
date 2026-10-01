import type { ReactNode } from 'react'

/** Units that stay on the same line as the number before them ("10 分钟", "12.4 MB", "8 hours"). */
const units = [
  'KB',
  'MB',
  'GB',
  'TB',
  'ms',
  's',
  'hr',
  'hours?',
  'minutes?',
  'days?',
  'seconds?',
  '分钟',
  '小时',
  '秒',
  '天',
  '个',
  '台',
  '种',
  '项',
  '核',
  '省',
  '列',
  '次',
  '条'
].join('|')

/**
 * Runs that must not break across lines: a platform with its version ("iOS 17+"), a number with its
 * unit, and hyphenated tokens. No lookbehind, which older Safari cannot parse.
 */
const unbreakable = new RegExp(
  [
    String.raw`(?<platform>\biOS \d+\+?)`,
    String.raw`(?<quantity>\d+(?:[.,]\d+)*\+?[ \u00a0](?:${units})(?![A-Za-z]))`,
    String.raw`(?<hyphenated>[@\w][\w@./+]*-[\w@./+-]*\w)`
  ].join('|'),
  'g'
)

/** Only identifier-like hyphenated tokens stay whole ("AGPL-3.0-or-later", "SHA-256"); "self-hosted" may break. */
const identifierLike = /[\d@./]/

/** A quantity right after one of these is part of a longer token, such as the "1.0" in "v1.0 版本". */
const tokenChar = /[\w.]/

/** Longer runs keep their break points, so they cannot overflow a phone-width column. */
const MAX_RUN = 24

function keepsWhole(text: string, match: RegExpMatchArray): boolean {
  const run = match[0]
  if (run.length > MAX_RUN) {
    return false
  }
  if (match.groups?.hyphenated) {
    return identifierLike.test(run)
  }
  if (match.groups?.quantity) {
    return !tokenChar.test(text.charAt((match.index ?? 0) - 1))
  }
  return true
}

/**
 * Prose with each unbreakable run wrapped in `<span class="nw">` (white-space: nowrap). Returns the
 * text unchanged when it has no such run. The spans add no characters, so copied text is unchanged.
 */
export function typeset(text: string): ReactNode {
  const parts: ReactNode[] = []
  let last = 0
  for (const match of text.matchAll(unbreakable)) {
    const start = match.index
    if (!keepsWhole(text, match)) {
      continue
    }
    if (start > last) {
      parts.push(text.slice(last, start))
    }
    parts.push(
      <span className="nw" key={start}>
        {match[0]}
      </span>
    )
    last = start + match[0].length
  }
  if (parts.length === 0) {
    return text
  }
  if (last < text.length) {
    parts.push(text.slice(last))
  }
  return parts
}
