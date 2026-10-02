import { count, type OramaPlugin, search } from '@orama/orama'
import { createFileRoute } from '@tanstack/react-router'
import { createFromSource } from 'fumadocs-core/search/server'

import { source } from '@/lib/source'

const wordSegmenter = new Intl.Segmenter('zh', { granularity: 'word' })
// ICU keeps `server.toml`, `1.2.3` and `com.example.cpu` as one word, so they are split here as Orama's English
// tokenizer splits them, and `toml` finds `server.toml` on the Chinese pages as it does on the English ones.
const wordSeparators = /[^\p{L}\p{N}_'-]+/u
const hanCharacter = /\p{Script=Han}/u
// Words that phrase a question or join the words it is about, rather than say what it is about. A Chinese query needs
// every word in one heading or paragraph, which rarely holds them, so 升级失败怎么办 and 配置文件在哪里 found nothing,
// and the 和 of 防火墙和告警 kept most firewall sections out of what it found. 办 and 样 are what ICU leaves of 怎么办 and
// 怎么样, and ICU keeps 参与, 以及, 及时 and 涉及 whole. 在 is not among them, as ICU splits 在线 into 在 and 线 (the 线 of
// 离线) and dropping it cut the word in half. ICU also splits 已用, 调用 and 复用 into a character and 用 (use), and
// 不可用 (unavailable) into 不可 and 用, so 用 is a filler word only where it follows neither a character alone nor a
// word ending in the 可 of 可用 (available), as in 可以用 Nginx 吗 and 用 Docker 部署.
const fillerWords = new Set(
  [
    '如何 怎么 怎样 咋 什么 啥 为什么 哪 哪里 哪儿 哪些 哪个 多久 多少 何时 办 样', // question words
    '是否 能否 能不能 可不可以 是不是 会不会', // yes-no questions
    '是 能 会 要 可以 请问', // the verbs a question is built with
    '吗 呢 吧 啊 呀 的 了', // particles
    '和 与 或 及 以及 或者 还是 并且 而且' // conjunctions
  ].flatMap((words) => words.split(' '))
)

// Words that ICU splits into characters, one of them a filler word, kept whole: the 和 (and) of 校验和 (checksum).
// Their pieces, less the filler word, are tokens too, so that 校验 (validation) still finds a checksum.
const wholeWords = ['校验和']
const wholeWord = new RegExp(wholeWords.join('|'), 'g')

/** The segments ICU finds in a text, each whole word being one. */
function* segments(text: string): Generator<Pick<Intl.SegmentData, 'isWordLike' | 'segment'>> {
  let start = 0
  for (const match of text.matchAll(wholeWord)) {
    yield* wordSegmenter.segment(text.slice(start, match.index))
    yield { isWordLike: true, segment: match[0] }
    start = match.index + match[0].length
  }
  yield* wordSegmenter.segment(text.slice(start))
}

/** Whether a word is a Chinese character alone. */
function isCharacter(word: string | undefined): boolean {
  return word?.length === 1 && hanCharacter.test(word)
}

/** The words ICU finds in a text, each marked as a filler word or not. */
function* segmentWords(text: string): Generator<{ filler: boolean; word: string }> {
  // The word right before, unless it is a filler word.
  let previous: string | undefined
  for (const { segment, isWordLike } of segments(text.normalize('NFKC').toLowerCase())) {
    if (!isWordLike) {
      previous = undefined
      continue
    }
    const filler = fillerWords.has(segment) || (segment === '用' && !isCharacter(previous) && !previous?.endsWith('可'))
    yield { filler, word: segment }
    previous = filler ? undefined : segment
  }
}

/** The words ICU finds in a text, less the filler words. */
function* chineseWords(text: string): Generator<string> {
  for (const { filler, word } of segmentWords(text)) {
    if (!filler) {
      yield word
    }
  }
}

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
    for (const segment of chineseWords(raw)) {
      for (const word of segment.split(wordSeparators)) {
        if (word) {
          tokens.add(word)
        }
      }
      if (wholeWords.includes(segment)) {
        for (const piece of wordSegmenter.segment(segment)) {
          if (piece.isWordLike && !fillerWords.has(piece.segment)) {
            tokens.add(piece.segment)
          }
        }
      }
    }
    return [...tokens]
  }
}

// What a reader typed between spaces, with a run of Chinese characters taken apart from the other characters typed
// against it, as in 卸载agent.
const typedWords = /\p{Script=Han}+|[^\s\p{Script=Han}]+/gu

/**
 * The words of a query. A character alone matches the start of any word, so a word of one character is left out, and
 * in a run of Chinese, where ICU splits the words it does not know into characters (卸载 into 卸 and 载, 区块链 into
 * 区块 and 链), each character stays with the word before it. A filler word ends a word, so the 卸载 of 服务端怎么卸载
 * is a word of its own.
 */
function queryWords(term: string): string[] {
  const words: string[][] = []
  for (const typed of term.match(typedWords) ?? []) {
    if (!hanCharacter.test(typed)) {
      words.push([typed])
      continue
    }
    // The word that a character alone joins: none at the start of the run or after a filler word.
    let previous: string[] | undefined
    for (const { filler, word } of segmentWords(typed)) {
      if (filler) {
        previous = undefined
      } else if (word.length === 1 && previous) {
        previous.push(word)
      } else {
        previous = [word]
        words.push(previous)
      }
    }
  }
  // A word is searched as it was typed, so that the 用 of 已用 still follows the character before it.
  return words.map((pieces) => pieces.join('')).filter((word) => chineseTokenizer.tokenize(word).join('').length > 1)
}

/** Groups hits as Orama does: by the values of `properties`, in the order they come, at most `maxResult` a group. */
function groupHits<Hit extends { document: object }>(
  hits: Hit[],
  { maxResult, properties }: { maxResult?: number; properties: string[] }
) {
  const groups = new Map<string, { result: Hit[]; values: unknown[] }>()
  for (const hit of hits) {
    const values = properties.map((property) => Reflect.get(hit.document, property))
    const key = JSON.stringify(values)
    let group = groups.get(key)
    if (!group) {
      group = { result: [], values }
      groups.set(key, group)
    }
    if (group.result.length < (maxResult || Number.POSITIVE_INFINITY)) {
      group.result.push(hit)
    }
  }
  return [...groups.values()]
}

// The searches the fallback runs, which it does not search again.
const wordSearches = new WeakSet<object>()

/**
 * Requiring every word of a Chinese query keeps one-character words, which match the start of any word, from finding
 * everything, but a query whose words sit in a heading and the paragraphs under it, such as 卸载 Agent, found nothing.
 * Such a query is searched again for each of its words, a word needing every piece ICU splits it into, and finds the
 * sections holding any of the words, as an English query does. Those holding more of the words come first, and those
 * holding as many by score: by score alone, a short section holding one word came before a long one holding two.
 * Searching it again for any of the pieces instead filled the results of 区块链 and 企业微信, which no page mentions,
 * with sections holding a word that starts with 链 or 信.
 */
const anyWordFallback: OramaPlugin = {
  name: 'any-word-fallback',
  async afterSearch(db, params, language, results) {
    if (params.mode === 'vector' || !params.term || results.count > 0 || wordSearches.has(params)) {
      return
    }
    // Each section found, with the number of words it holds.
    const matches = new Map<string, { hit: (typeof results.hits)[number]; words: number }>()
    for (const word of queryWords(params.term)) {
      const wordSearch = { ...params, groupBy: undefined, limit: count(db), offset: 0, term: word }
      wordSearches.add(wordSearch)
      for (const hit of (await search(db, wordSearch, language)).hits) {
        const match = matches.get(hit.id)
        matches.set(hit.id, {
          hit: { ...hit, score: hit.score + (match?.hit.score ?? 0) },
          words: (match?.words ?? 0) + 1
        })
      }
    }
    const hits = [...matches.values()]
      .sort((a, b) => b.words - a.words || b.hit.score - a.hit.score)
      .map(({ hit }) => hit)
    const { groupBy, limit = 10, offset = 0 } = params
    Object.assign(results, {
      count: hits.length,
      groups: groupBy && groupHits(hits, groupBy),
      hits: hits.slice(offset, offset + limit)
    })
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
