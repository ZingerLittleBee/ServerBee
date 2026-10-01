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

const hrefAttribute = /\bhref="([^"]*)"/

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

const missingExports = ['/en/docs/nope.mdx', '/zh/docs/nope.mdx', '/fr/docs/quick-start.mdx', '/llms.mdx/docs/nope']

// Each query must find its page, and only pages in the requested language.
const searches = [
  { locale: 'en', query: 'firewall', page: '/en/docs/firewall' },
  { locale: 'en', query: 'alert', page: '/en/docs/alerts' },
  { locale: 'zh', query: '防火墙', page: '/zh/docs/firewall' },
  { locale: 'zh', query: '安装', page: '/zh/docs/quick-start' },
  { locale: 'zh', query: 'Docker 安装', page: '/zh/docs/quick-start' },
  // A question word the pages do not contain does not empty the results.
  { locale: 'zh', query: '如何安装', page: '/zh/docs/quick-start' },
  // English words inside Chinese prose match case-insensitively.
  { locale: 'zh', query: 'websocket', page: '/zh/docs/api-reference' },
  // Full-width Latin, which Chinese input methods can produce.
  { locale: 'zh', query: 'ｗｅｂｓｏｃｋｅｔ', page: '/zh/docs/api-reference' },
  // `server.toml` is split as on the English pages, so its parts still match.
  { locale: 'zh', query: 'toml', page: '/zh/docs/deployment' }
] as const

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
  if (route.path.includes('/docs')) {
    const missing = docsLabels[route.lang].filter((label) => !html.includes(label))
    if (missing.length > 0) {
      throw new Error(`${route.path} did not render the labels: ${missing.join(', ')}`)
    }
    const titles = [...html.matchAll(/<title>([^<]*)<\/title>/g)].map((match) => match[1])
    expect(
      titles.length === 1 && titles[0].endsWith(` | ${siteNames[route.lang]}`),
      `${route.path} has the titles ${titles.join(', ') || '<none>'}`
    )
    expect(html.includes('<meta name="description" content="'), `${route.path} has no description`)
    const pagePath = route.path.slice(route.lang.length + 1)
    for (const link of [
      `<link rel="canonical" href="https://docs.serverbee.app${route.path}"/>`,
      `hrefLang="en" href="https://docs.serverbee.app/en${pagePath}"`,
      `hrefLang="zh-Hans" href="https://docs.serverbee.app/zh${pagePath}"`,
      `hrefLang="x-default" href="https://docs.serverbee.app/en${pagePath}"`,
      `property="og:image" content="https://docs.serverbee.app/og/landing-${route.lang}.png"`,
      `type="text/markdown" href="${route.path}.mdx"`
    ]) {
      expect(html.includes(link), `${route.path} lacks ${link}`)
    }
  }
}

for (const search of searches) {
  const response = await fetch(
    `${baseUrl}/api/search?query=${encodeURIComponent(search.query)}&locale=${search.locale}`
  )
  const results = (await response.json()) as { content: string; type: string; url: string }[]
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
}

// fumadocs links go through src/components/framework-link.tsx. A link to a heading on the same page stays a fragment,
// which the router would otherwise turn into the path /<page>/#<heading>.
for (const path of ['/en/docs/deployment', '/zh/docs/deployment']) {
  const html = await (await fetch(`${baseUrl}${path}`)).text()
  expect(!html.includes(`href="${path}/#`), `${path} renders a same-page heading link as a path`)
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

// A missing page answers 404 in the language of its URL, titled and linked within that language.
for (const { path, lang, heading } of [
  { path: '/en/docs/nope', lang: 'en', heading: 'Page not found' },
  { path: '/zh/docs/nope', lang: 'zh', heading: '页面不存在' },
  { path: '/zh/nope', lang: 'zh', heading: '页面不存在' }
] as const) {
  const response = await fetch(`${baseUrl}${path}`, { redirect: 'manual' })
  expect(response.status === 404, `${path} returned ${response.status} instead of 404`)
  const html = await response.text()
  expect(html.includes(`<html lang="${lang}">`), `${path} did not render lang=${lang}`)
  expect(html.includes(heading), `${path} did not render "${heading}"`)
  expect(html.includes(`<title>${siteNames[lang]}</title>`), `${path} is not titled ${siteNames[lang]}`)
  for (const href of [`/${lang}`, `/${lang}/docs`]) {
    expect(html.includes(`href="${href}"`), `${path} does not link ${href}`)
  }
  expect(!html.includes('aria-current="page"'), `${path} marks a link as the current page`)
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

console.log(
  `PASS: ${routes.length} localized documentation routes, ${searches.length} search queries, ${markdownExports.length} Markdown exports, llms.txt in both languages, localized 404 pages`
)
