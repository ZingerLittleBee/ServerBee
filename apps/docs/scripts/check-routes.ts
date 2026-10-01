import { setTimeout as delay } from 'node:timers/promises'

const baseUrl = process.env.SERVERBEE_DOCS_BASE_URL ?? 'http://127.0.0.1:4000'

const routes = [
  { path: '/en', lang: 'en', marker: 'Self-hosted VPS monitoring' },
  { path: '/zh', lang: 'zh', marker: '自托管的 VPS 监控' },
  { path: '/en/docs', lang: 'en', marker: 'ServerBee has two core components' },
  { path: '/zh/docs', lang: 'zh', marker: 'ServerBee 由两个核心组件构成' },
  { path: '/en/docs/quick-start', lang: 'en', marker: 'Choose a deployment method' },
  { path: '/zh/docs/quick-start', lang: 'zh', marker: '先选择部署方式' },
  { path: '/en/docs/configuration', lang: 'en', marker: 'Configuration Loading Priority' },
  { path: '/zh/docs/configuration', lang: 'zh', marker: '配置加载优先级' }
] as const

// Labels every docs page renders. fumadocs-ui hard-codes most of them in English, and patches/fumadocs-ui@16.6.16.patch
// routes them through the translations in src/routes/__root.tsx.
const docsLabels = {
  en: [
    'On this page',
    'Choose a language',
    'Open Search',
    'Open Sidebar',
    'Collapse Sidebar',
    'Toggle Theme',
    'Copy Markdown'
  ],
  zh: ['本页目录', '选择语言', '打开搜索', '打开侧边栏', '收起侧边栏', '切换主题', '复制 Markdown']
} as const

// A Chinese page names its controls in Chinese, apart from the GitHub link: fumadocs-ui hard-codes names in English,
// and Radix names the 404 page's header "Main".
const ariaLabelAttribute = /\baria-label="([^"]*)"/g
const hanCharacter = /\p{Script=Han}/u
// fumadocs marks the query's words in the results it returns.
const highlightTags = /<\/?mark>/g

function englishLabels(html: string): string[] {
  return [...html.matchAll(ariaLabelAttribute)]
    .map((match) => match[1])
    .filter((label) => label !== 'GitHub' && !hanCharacter.test(label))
}

const hrefAttribute = /\bhref="([^"]*)"/

// Token colors of the default code themes under 4.5:1 on the code block backgrounds, replaced in source.config.ts.
const lowContrastTokens = /--shiki-light:#(?:6a737d|d73a49|22863a|e36209)\b|--shiki-dark:#6a737d\b/i

const siteNames = { en: 'ServerBee Docs', zh: 'ServerBee 文档' } as const

// Markdown exports, each in its page's language with its section headings, and served as UTF-8.
const markdownExports = [
  { path: '/en/docs.mdx', title: '# Introduction', heading: '\n## What Is ServerBee' },
  { path: '/zh/docs.mdx', title: '# 介绍', heading: '\n## ServerBee 是什么' },
  { path: '/en/docs/quick-start.mdx', title: '# Quick Install', heading: '\n## Choose a deployment method' },
  { path: '/zh/docs/quick-start.mdx', title: '# 快速安装', heading: '\n## 先选择部署方式' },
  // The first export URL, which deployed pages linked to before the export was localized.
  { path: '/llms.mdx/docs/quick-start', title: '# Quick Install', heading: '\n## Choose a deployment method' }
] as const

const missingExports = [
  '/en/docs/nope.mdx',
  '/zh/docs/nope.mdx',
  '/fr/docs/quick-start.mdx',
  '/llms.mdx/docs/nope',
  // Slugs holding a `%`, which answered 500 when decoded again, or found the quick start (%2571uick-start).
  '/en/docs/x%25y.mdx',
  '/llms.mdx/docs/x%25y',
  '/en/docs/%2571uick-start.mdx'
]

interface SearchCheck {
  /** Words that every section found holds. */
  every?: string[]
  /** Text that the first section found holds. */
  first?: string
  locale: 'en' | 'zh'
  page: string
  query: string
  /** A query that finds the same results. */
  same?: string
}

interface SearchResult {
  content: string
  id: string
  type: string
  url: string
}

// Each query must find its page, and only pages in the requested language.
const searches: SearchCheck[] = [
  { locale: 'en', query: 'firewall', page: '/en/docs/firewall' },
  { locale: 'en', query: 'alert', page: '/en/docs/alerts' },
  { locale: 'zh', query: '防火墙', page: '/zh/docs/firewall' },
  { locale: 'zh', query: '安装', page: '/zh/docs/quick-start' },
  { locale: 'zh', query: 'Docker 安装', page: '/zh/docs/quick-start' },
  // Question words, which the pages rarely contain, do not empty the results, and the sections found hold every other
  // word of the question.
  { locale: 'zh', query: '如何安装', page: '/zh/docs/quick-start' },
  { locale: 'zh', query: '升级失败怎么办', page: '/zh/docs/deployment', every: ['升级', '失败'] },
  { locale: 'zh', query: '配置文件在哪里', page: '/zh/docs/configuration', every: ['配置', '文件'] },
  // Words that no heading or paragraph holds together, here a heading and the paragraphs under it, with or without a
  // space between them.
  { locale: 'zh', query: '卸载 Agent', page: '/zh/docs/deployment' },
  { locale: 'zh', query: '卸载agent', page: '/zh/docs/deployment' },
  { locale: 'zh', query: 'Agent卸载', page: '/zh/docs/deployment' },
  // A conjunction joins the words a query is about, and is not one of them.
  { locale: 'zh', query: '防火墙和告警', page: '/zh/docs/firewall', same: '防火墙告警' },
  // English words inside Chinese prose match case-insensitively.
  { locale: 'zh', query: 'websocket', page: '/zh/docs/api-reference' },
  // Full-width Latin, which Chinese input methods can produce.
  { locale: 'zh', query: 'ｗｅｂｓｏｃｋｅｔ', page: '/zh/docs/api-reference' },
  // `server.toml` is split as on the English pages, so its parts still match, and a section must hold both of them,
  // not one of them twice.
  { locale: 'zh', query: 'toml', page: '/zh/docs/deployment' },
  { locale: 'zh', query: 'server.toml', page: '/zh/docs/configuration', every: ['server', 'toml'] },
  // 用 is a word of its own: ICU leaves it alone in 已用, 调用 and 复用, and sections holding the term come first.
  { locale: 'zh', query: '已用', page: '/zh/docs/alerts', first: '已用' }
]

// Words that no page holds find nothing, though ICU splits them into words that pages do hold: 区块 and 链, and 企业, 微
// and 信.
const misses = ['区块链', '企业微信']

async function waitUntilReady(): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/en`, { redirect: 'manual' })
      if (response.status === 200) {
        return
      }
    } catch {
      // The production server may still be starting.
    }
    await delay(200)
  }
  throw new Error(`Documentation server did not become ready at ${baseUrl}`)
}

/** A Content-Type compared without case or spaces: text/plain;charset=UTF-8 equals text/plain; charset=utf-8. */
function contentType(response: Response): string {
  return (response.headers.get('content-type') ?? '').replace(/\s/g, '').toLowerCase()
}

async function searchFor(query: string, locale: string): Promise<SearchResult[]> {
  const response = await fetch(`${baseUrl}/api/search?query=${encodeURIComponent(query)}&locale=${locale}`)
  return (await response.json()) as SearchResult[]
}

function expect(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message)
  }
}

await waitUntilReady()

for (const route of routes) {
  const response = await fetch(`${baseUrl}${route.path}`, { redirect: 'manual' })
  if (response.status !== 200) {
    throw new Error(
      `${route.path} returned ${response.status} with Location=${response.headers.get('location') ?? '<none>'}`
    )
  }
  const html = await response.text()
  if (!html.includes(`<html lang="${route.lang}">`)) {
    throw new Error(`${route.path} did not render lang=${route.lang}`)
  }
  if (!html.includes(route.marker)) {
    throw new Error(`${route.path} did not render its expected localized content`)
  }
  // fumadocs' docs layout has no main landmark (patches/fumadocs-ui@16.6.16.patch gives its article the role).
  expect(html.match(/<main\b|role="main"/g)?.length === 1, `${route.path} does not have exactly one main landmark`)
  // An svg exposed as an image needs a name, which fumadocs' GitHub link icon lacked (src/lib/layout.shared.tsx).
  expect(!/<svg\b[^>]*\brole="img"[^>]*>(?!<title>)/.test(html), `${route.path} renders an unnamed role="img" svg`)
  if (route.path.includes('/docs')) {
    const missing = docsLabels[route.lang].filter((label) => !html.includes(label))
    if (missing.length > 0) {
      throw new Error(`${route.path} did not render the labels: ${missing.join(', ')}`)
    }
    const english = route.lang === 'zh' ? englishLabels(html) : []
    expect(english.length === 0, `${route.path} names controls in English: ${english.join(', ')}`)
    const titles = [...html.matchAll(/<title>([^<]*)<\/title>/g)].map((match) => match[1])
    expect(
      titles.length === 1 && titles[0].endsWith(` | ${siteNames[route.lang]}`),
      `${route.path} has the titles ${titles.join(', ') || '<none>'}`
    )
    expect(html.includes('<meta name="description" content="'), `${route.path} has no description`)
    expect(!lowContrastTokens.test(html), `${route.path} renders a code token color under 4.5:1`)
    const pagePath = route.path.slice(route.lang.length + 1)
    for (const link of [
      `<link rel="canonical" href="https://docs.serverbee.app${route.path}"/>`,
      `hrefLang="en" href="https://docs.serverbee.app/en${pagePath}"`,
      `hrefLang="zh-Hans" href="https://docs.serverbee.app/zh${pagePath}"`,
      `hrefLang="x-default" href="https://docs.serverbee.app/en${pagePath}"`,
      `property="og:image" content="https://docs.serverbee.app/og/landing-${route.lang}.png"`,
      'property="og:image:alt" content="ServerBee',
      'name="twitter:image:alt" content="ServerBee',
      `type="text/markdown" href="${route.path}.mdx"`
    ]) {
      expect(html.includes(link), `${route.path} lacks ${link}`)
    }
  }
}

for (const search of searches) {
  const results = await searchFor(search.query, search.locale)
  const pages = results.filter((result) => result.type === 'page').map((result) => result.url)
  if (!pages.includes(search.page)) {
    throw new Error(`Search for "${search.query}" (${search.locale}) did not find ${search.page}`)
  }
  const foreign = results.find((result) => !result.url.startsWith(`/${search.locale}/docs`))
  if (foreign) {
    throw new Error(`Search for "${search.query}" (${search.locale}) returned ${foreign.url}`)
  }
  // A component indexed as its source opens the page holding it, not the page it describes.
  const rawJsx = results.find((result) => result.content.includes('<Card'))
  if (rawJsx) {
    throw new Error(`Search for "${search.query}" (${search.locale}) returned raw JSX at ${rawJsx.url}`)
  }
  const sections = results.filter((result) => result.type !== 'page')
  const partial = sections.find((section) =>
    search.every?.some((word) => !section.content.replace(highlightTags, '').toLowerCase().includes(word))
  )
  if (partial) {
    throw new Error(
      `Search for "${search.query}" (${search.locale}) returned a section without all its words: ${partial.url}`
    )
  }
  const first = sections[0]?.content.replace(highlightTags, '')
  if (search.first && !first?.includes(search.first)) {
    throw new Error(`Search for "${search.query}" (${search.locale}) returned first a section without ${search.first}`)
  }
  if (search.same) {
    const ids = (list: SearchResult[]) => list.map((result) => result.id).join('\n')
    expect(
      ids(results) === ids(await searchFor(search.same, search.locale)),
      `Search for "${search.query}" (${search.locale}) did not find what "${search.same}" finds`
    )
  }
}

for (const query of misses) {
  const results = await searchFor(query, 'zh')
  expect(results.length === 0, `Search for "${query}" (zh) found ${results[0]?.url}`)
}

// fumadocs links go through src/components/framework-link.tsx. A link to a heading on the same page stays a fragment,
// which the router would otherwise turn into the path /<page>/#<heading>. The checkboxes of the deployment checklist
// are named by the text after them (src/components/list-item.tsx).
for (const path of ['/en/docs/deployment', '/zh/docs/deployment']) {
  const html = await (await fetch(`${baseUrl}${path}`)).text()
  expect(!html.includes(`href="${path}/#`), `${path} renders a same-page heading link as a path`)
  const tasks = html.split('<li class="task-list-item">').slice(1)
  expect(
    tasks.length > 0 && tasks.every((task) => task.startsWith('<label><input type="checkbox"')),
    `${path} renders a task list checkbox without a label`
  )
}

// Only the page itself is the current page: not the home and docs index links above it, and not for another query.
for (const path of ['/en/docs/quick-start', '/zh/docs/quick-start?ref=github', '/en/docs']) {
  const html = await (await fetch(`${baseUrl}${path}`)).text()
  const current = [...html.matchAll(/<a\b[^>]*\baria-current="page"[^>]*>/g)].map(
    (match) => match[0].match(hrefAttribute)?.[1]
  )
  const pathname = path.split('?')[0]
  expect(
    current.length > 0 && current.every((href) => href === pathname),
    `${path} marks ${current.join(', ') || 'no link'} as the current page`
  )
}

// A missing page answers 404 in the language of its URL, titled and linked within that language: a missing docs page
// (also one whose slug holds a `%`), a path under a language that no route matches, and an unknown language.
for (const { path, lang, heading } of [
  { path: '/en/docs/nope', lang: 'en', heading: 'Page not found' },
  { path: '/en/docs/x%25y', lang: 'en', heading: 'Page not found' },
  { path: '/en/docs/%2571uick-start', lang: 'en', heading: 'Page not found' },
  { path: '/zh/docs/nope', lang: 'zh', heading: '页面不存在' },
  { path: '/zh/nope', lang: 'zh', heading: '页面不存在' },
  { path: '/nope', lang: 'en', heading: 'Page not found' }
] as const) {
  const response = await fetch(`${baseUrl}${path}`, { redirect: 'manual' })
  expect(response.status === 404, `${path} returned ${response.status} instead of 404`)
  const html = await response.text()
  expect(html.includes(`<html lang="${lang}">`), `${path} did not render lang=${lang}`)
  expect(html.includes(heading), `${path} did not render "${heading}"`)
  const title = `${heading} | ${siteNames[lang]}`
  expect(html.includes(`<title>${title}</title>`), `${path} is not titled ${title}`)
  for (const href of [`/${lang}`, `/${lang}/docs`]) {
    expect(html.includes(`href="${href}"`), `${path} does not link ${href}`)
  }
  expect(!html.includes('aria-current="page"'), `${path} marks a link as the current page`)
  const english = lang === 'zh' ? englishLabels(html) : []
  expect(english.length === 0, `${path} names controls in English: ${english.join(', ')}`)
}

for (const route of markdownExports) {
  // HEAD needs its own handler, or it gets the HTML page's headers.
  for (const method of ['GET', 'HEAD'] as const) {
    const response = await fetch(`${baseUrl}${route.path}`, { method, redirect: 'manual' })
    expect(response.status === 200, `${method} ${route.path} returned ${response.status}`)
    expect(
      contentType(response) === 'text/markdown;charset=utf-8',
      `${method} ${route.path} returned Content-Type ${response.headers.get('content-type')}`
    )
    if (method === 'GET') {
      const markdown = await response.text()
      expect(markdown.startsWith(`${route.title}\n`), `${route.path} does not start with "${route.title}"`)
      expect(markdown.includes(route.heading), `${route.path} lost its section headings`)
      expect(!markdown.includes('&#x2A;'), `${route.path} encodes an emphasis marker as &#x2A;`)
    }
  }
}

for (const path of missingExports) {
  for (const method of ['GET', 'HEAD'] as const) {
    const response = await fetch(`${baseUrl}${path}`, { method, redirect: 'manual' })
    expect(response.status === 404, `${method} ${path} returned ${response.status} instead of 404`)
  }
}

// Old and language-less URLs keep their page and query. A first segment that names no language is a 404.
for (const { path, status, to } of [
  { path: '/?ref=github', status: 307, to: '/en?ref=github' },
  { path: '/docs/quick-start?ref=github', status: 307, to: '/en/docs/quick-start?ref=github' },
  // The Markdown export before it was localized.
  { path: '/docs/quick-start.mdx?ref=github', status: 308, to: '/en/docs/quick-start.mdx?ref=github' },
  // The Chinese pages were published under /cn before the locale was renamed zh.
  { path: '/cn/docs/configuration', status: 308, to: '/zh/docs/configuration' },
  { path: '/zh-CN/docs/quick-start?ref=github', status: 308, to: '/zh/docs/quick-start?ref=github' },
  // Redirecting to the decoded path puts a character in the Location header that fails the request.
  { path: '/zh-CN/docs/%E5%AE%89%E8%A3%85', status: 308, to: '/zh/docs/%E5%AE%89%E8%A3%85' },
  { path: '/EN/docs', status: 308, to: '/en/docs' },
  // A query comes back as it was, where JSON would read 1.10 as 1.1 and "x" as x.
  { path: '/en/docs/quick-start?v=1.10&q=%22x%22', status: 200 },
  { path: '/docs/quick-start?v=1.10', status: 307, to: '/en/docs/quick-start?v=1.10' },
  { path: '/nope', status: 404 },
  { path: '/fr/docs/quick-start', status: 404 },
  { path: '/constructor/docs', status: 404 },
  { path: '/apple-touch-icon.png', status: 404 }
]) {
  const response = await fetch(`${baseUrl}${path}`, { redirect: 'manual' })
  const location = response.headers.get('location')
  const target = location === null ? undefined : new URL(location, baseUrl)
  expect(
    response.status === status && (to === undefined || `${target?.pathname}${target?.search}` === to),
    `${path} returned ${response.status} to ${location ?? '<none>'}`
  )
}

// The router redirected a path holding one of these characters to the path with it unencoded, which the next request
// encoded again, until the browser gave up.
for (const character of ['"', '<', '>', '^', '`', '{', '}']) {
  const path = `/en/docs/x${encodeURIComponent(character)}y`
  const response = await fetch(`${baseUrl}${path}`, { redirect: 'manual' })
  expect(
    response.status === 404,
    `${path} returned ${response.status} to ${response.headers.get('location') ?? '<none>'}`
  )
}

// Each llms.txt (https://llmstxt.org) lists its own language's pages, linking their Markdown exports.
for (const { path, lang, other } of [
  { path: '/llms.txt', lang: 'en', other: 'zh' },
  { path: '/zh/llms.txt', lang: 'zh', other: 'en' }
]) {
  for (const method of ['GET', 'HEAD'] as const) {
    const response = await fetch(`${baseUrl}${path}`, { method })
    expect(response.status === 200, `${method} ${path} returned ${response.status}`)
    expect(
      contentType(response) === 'text/plain;charset=utf-8',
      `${method} ${path} returned Content-Type ${response.headers.get('content-type')}`
    )
  }
  const index = await (await fetch(`${baseUrl}${path}`)).text()
  expect(index.match(/^# /gm)?.length === 1, `${path} must have exactly one H1`)
  expect(
    /^# ServerBee\n\n> .+\n\n[^#\n]/.test(index),
    `${path} does not open with the project name, a summary and a note`
  )
  expect(/^## /m.test(index), `${path} has no H2 link sections`)
  const pageLinks = [...index.matchAll(/\]\((https:\/\/docs\.serverbee\.app\/(en|zh)\/docs[^)]*)\)/g)]
  expect(pageLinks.length > 0, `${path} links no pages with absolute URLs`)
  expect(
    pageLinks.every((link) => link[2] === lang && link[1].endsWith('.mdx')),
    `${path} links a page outside ${lang} or not to its Markdown export`
  )
  expect(!index.includes(`](/${other}/docs`), `${path} mixes in ${other} pages`)
}
expect((await fetch(`${baseUrl}/fr/llms.txt`)).status === 404, '/fr/llms.txt is not a 404')

// Each llms-full.txt holds one language's pages in navigation order, each with its URL.
for (const { path, first, foreign } of [
  { path: '/llms-full.txt', first: '# Introduction\n', foreign: '\n# 快速安装\n' },
  { path: '/zh/llms-full.txt', first: '# 介绍\n', foreign: '\n# Quick Install\n' }
]) {
  for (const method of ['GET', 'HEAD'] as const) {
    const response = await fetch(`${baseUrl}${path}`, { method })
    expect(response.status === 200, `${method} ${path} returned ${response.status}`)
    expect(
      contentType(response) === 'text/plain;charset=utf-8',
      `${method} ${path} returned Content-Type ${response.headers.get('content-type')}`
    )
  }
  const full = await (await fetch(`${baseUrl}${path}`)).text()
  expect(full.startsWith(first), `${path} does not start with the introduction`)
  expect(!full.includes(foreign), `${path} contains pages of the other language`)
  expect(full.includes('\nURL: https://docs.serverbee.app/'), `${path} does not give the page URLs`)
}
expect((await fetch(`${baseUrl}/fr/llms-full.txt`)).status === 404, '/fr/llms-full.txt is not a 404')

// The sitemap lists the landing and every docs page in each language, each with its hreflang alternates, and only
// URLs that answer 200: a redirect or a 404 in a sitemap is a crawl error.
for (const method of ['GET', 'HEAD'] as const) {
  const response = await fetch(`${baseUrl}/sitemap.xml`, { method })
  expect(response.status === 200, `${method} /sitemap.xml returned ${response.status}`)
  expect(
    contentType(response) === 'application/xml;charset=utf-8',
    `${method} /sitemap.xml returned Content-Type ${response.headers.get('content-type')}`
  )
}
const sitemap = await (await fetch(`${baseUrl}/sitemap.xml`)).text()
const entries = [...sitemap.matchAll(/<url><loc>([^<]*)<\/loc>(.*?)<\/url>/g)]
const sitemapUrls = entries.map((entry) => entry[1])
// The pages of each language, as llms.txt links their Markdown exports.
for (const { lang, index } of [
  { lang: 'en', index: '/llms.txt' },
  { lang: 'zh', index: '/zh/llms.txt' }
]) {
  const llms = await (await fetch(`${baseUrl}${index}`)).text()
  const pages = [...llms.matchAll(/\]\((https:\/\/docs\.serverbee\.app\/[^)]*)\.mdx\)/g)].map((match) => match[1])
  const listed = sitemapUrls.filter((url) => url.startsWith(`https://docs.serverbee.app/${lang}/docs`))
  expect(
    pages.length > 0 && listed.length === pages.length && pages.every((page) => listed.includes(page)),
    `/sitemap.xml lists ${listed.length} ${lang} docs pages, ${index} links ${pages.length}`
  )
  expect(sitemapUrls.includes(`https://docs.serverbee.app/${lang}`), `/sitemap.xml does not list the ${lang} landing`)
}
expect(
  entries.every((entry) => ['en', 'zh-Hans', 'x-default'].every((code) => entry[2].includes(`hreflang="${code}"`))),
  '/sitemap.xml has an entry without its hreflang alternates'
)
const unreachable: string[] = []
for (let start = 0; start < sitemapUrls.length; start += 8) {
  await Promise.all(
    sitemapUrls.slice(start, start + 8).map(async (url) => {
      const response = await fetch(url.replace('https://docs.serverbee.app', baseUrl), { redirect: 'manual' })
      if (response.status !== 200) {
        unreachable.push(`${url} (${response.status})`)
      }
    })
  )
}
expect(unreachable.length === 0, `/sitemap.xml lists URLs that do not answer 200: ${unreachable.join(', ')}`)
const robots = await (await fetch(`${baseUrl}/robots.txt`)).text()
expect(
  robots.split('\n').includes('Sitemap: https://docs.serverbee.app/sitemap.xml'),
  'robots.txt does not point to the sitemap'
)

console.log(
  `PASS: ${routes.length} localized documentation routes, ${searches.length + misses.length} search queries, ${markdownExports.length} Markdown exports, llms.txt in both languages, localized 404 pages, ${sitemapUrls.length} sitemap URLs`
)
