/* How a heading is cut into the pieces that arrive one after another. */

import type { Lang } from './site'

/** Chinese arrives a character at a time. A Latin run stays whole, and punctuation rides with what it follows. */
const ZH_PIECE = /[A-Za-z0-9@.+\-/]+[，。、！？：；,.!?]*|\s+|.[，。、！？：；]*/gsu
/** English arrives a word at a time. */
const EN_PIECE = /\S+|\s+/gu

/** The pieces of a heading in reading order, whitespace included as pieces of its own. */
export function pieces(text: string, lang: Lang): string[] {
  return text.match(lang === 'zh' ? ZH_PIECE : EN_PIECE) ?? []
}

/** How many of a heading's pieces are visible, which is how far it moves the stagger along. */
export function pieceCount(text: string, lang: Lang): number {
  return pieces(text, lang).filter((piece) => piece.trim()).length
}
