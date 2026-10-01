import { type OramaPlugin, search } from '@orama/orama'
import { createFileRoute } from '@tanstack/react-router'
import { createFromSource } from 'fumadocs-core/search/server'

import { source } from '@/lib/source'

const wordSegmenter = new Intl.Segmenter('zh', { granularity: 'word' })
// ICU keeps `server.toml`, `1.2.3` and `com.example.cpu` as one word, so they are split here as Orama's English
// tokenizer splits them, and `toml` finds `server.toml` on the Chinese pages as it does on the English ones.
const wordSeparators = /[^\p{L}\p{N}_'-]+/u
// Words that phrase a question rather than say what it is about. A Chinese query needs every word in one heading or
// paragraph, which rarely holds them, so 升级失败怎么办 and 配置文件在哪里 found nothing. 办 and 样 are what ICU leaves
// of 怎么办 and 怎么样. 在 is not one of them: ICU splits 在线 into 在 and 线, the 线 of 离线.
const fillerWords = new Set(
  [
    '如何 怎么 怎样 咋 什么 啥 为什么 哪 哪里 哪儿 哪些 哪个 多久 多少 何时 办 样', // question words
    '是否 能否 能不能 可不可以 是不是 会不会', // yes-no questions
    '是 能 会 要 可以 用 请问', // the verbs a question is built with
    '吗 呢 吧 啊 呀 的 了' // particles
  ].flatMap((words) => words.split(' '))
)

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

/**
 * Requiring every word of a Chinese query keeps one-character words, which match the start of any word, from finding
 * everything, but a query whose words sit in a heading and the paragraphs under it, such as 卸载 Agent, found nothing.
 * Such a query is searched again for any of its words, which is how English queries are searched.
 */
const anyWordFallback: OramaPlugin = {
  name: 'any-word-fallback',
  async afterSearch(db, params, language, results) {
    // A vector search has no words to require, and the search below, which needs any word, is not searched again.
    if (params.mode === 'vector' || params.threshold !== 0 || results.count > 0) {
      return
    }
    const { count, groups, hits } = await search(db, { ...params, threshold: 1 }, language)
    Object.assign(results, { count, groups, hits })
  }
}

const server = createFromSource(source, {
  localeMap: {
    en: { language: 'english' },
    zh: {
      components: { tokenizer: chineseTokenizer },
      plugins: [anyWordFallback],
      // What fumadocs documents for Chinese: a result must contain every word of the query, and no typo tolerance,
      // which would let a two-character word match any word sharing one character with it. Orama counted a query word
      // once for each word it starts, so for `server.toml` a section holding `server` and `serverbee` counted as one
      // holding both words; patches/@orama%2Forama@3.1.18.patch counts each query word once.
      search: { threshold: 0, tolerance: 0 }
    }
  }
})

const GET = ({ request }: { request: Request }) => server.GET(request)

export const Route = createFileRoute('/api/search')({
  // Without HEAD the request falls through to the app router, which answers with an HTML page.
  server: { handlers: { GET, HEAD: GET } }
})
