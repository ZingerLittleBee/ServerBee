/* FAQ entries for native <details>; only the first starts open. */

import { type LandingLang, landingCopy } from '../translations'

interface FaqItem {
  a: string
  /** The question, unique within the list. */
  id: string
  open: boolean
  q: string
}

export function buildFaq(lang: LandingLang): FaqItem[] {
  return landingCopy[lang].faq.items.map((item, index) => ({ id: item.q, ...item, open: index === 0 }))
}
