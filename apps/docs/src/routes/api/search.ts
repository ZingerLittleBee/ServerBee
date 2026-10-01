import { createFileRoute } from '@tanstack/react-router'
import { createFromSource } from 'fumadocs-core/search/server'

import { source } from '@/lib/source'

const wordSegmenter = new Intl.Segmenter('zh', { granularity: 'word' })
// ICU keeps `server.toml`, `1.2.3` and `com.example.cpu` as one word, so they are split here as Orama's English
// tokenizer splits them, and `toml` finds `server.toml` on the Chinese pages as it does on the English ones.
const wordSeparators = /[^\p{L}\p{N}_'-]+/u
// Every word of a Chinese query must match, so question words and particles that the pages rarely contain would
// leave a question such as 如何安装 with no results.
const fillerWords = new Set(['如何', '怎么', '怎样', '什么', '为什么', '哪些', '哪个', '是否', '吗', '呢', '的', '了'])

/**
 * Orama's built-in tokenizers keep only Latin letters, so Chinese text produced no tokens and no Chinese query could
 * match. ICU word segmentation (the engine @orama/tokenizers/mandarin also uses) splits Chinese and the English terms
 * mixed into it. NFKC folds the full-width letters and digits Chinese input methods produce, and lowercasing keeps
 * `websocket` matching `WebSocket`, which that package does not do.
 */
const chineseTokenizer = {
  language: 'mandarin',
  normalizationCache: new Map<string, string>(),
  tokenize(raw: string): string[] {
    const tokens = new Set<string>()
    for (const { segment, isWordLike } of wordSegmenter.segment(raw.normalize('NFKC').toLowerCase())) {
      if (!isWordLike || fillerWords.has(segment)) {
        continue
      }
      for (const word of segment.split(wordSeparators)) {
        if (word) {
          tokens.add(word)
        }
      }
    }
    return [...tokens]
  }
}

const server = createFromSource(source, {
  localeMap: {
    en: { language: 'english' },
    zh: {
      components: { tokenizer: chineseTokenizer },
      // What fumadocs documents for Chinese: a result must contain every word of the query, and no typo tolerance,
      // which would let a two-character word match any word sharing one character with it.
      search: { threshold: 0, tolerance: 0 }
    }
  }
})

const GET = ({ request }: { request: Request }) => server.GET(request)

export const Route = createFileRoute('/api/search')({
  // Without HEAD the request falls through to the app router, which answers with an HTML page.
  server: { handlers: { GET, HEAD: GET } }
})
