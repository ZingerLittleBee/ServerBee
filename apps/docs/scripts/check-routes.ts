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

// Markdown exports, each in its page's language and served as UTF-8.
const markdownExports = [
  { path: '/en/docs.mdx', title: '# Introduction' },
  { path: '/zh/docs.mdx', title: '# 介绍' },
  { path: '/en/docs/quick-start.mdx', title: '# Quick Install' },
  { path: '/zh/docs/quick-start.mdx', title: '# 快速安装' },
  // The first export URL, which deployed pages linked to before the export was localized.
  { path: '/llms.mdx/docs/quick-start', title: '# Quick Install' }
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
    }
  }
}

for (const path of missingExports) {
  for (const method of ['GET', 'HEAD'] as const) {
    const response = await fetch(`${baseUrl}${path}`, { method, redirect: 'manual' })
    expect(response.status === 404, `${method} ${path} returned ${response.status} instead of 404`)
  }
}

const legacy = await fetch(`${baseUrl}/docs/quick-start.mdx`, { redirect: 'manual' })
expect(
  legacy.status === 308 &&
    new URL(legacy.headers.get('location') ?? '', baseUrl).pathname === '/en/docs/quick-start.mdx',
  `/docs/quick-start.mdx returned ${legacy.status} to ${legacy.headers.get('location') ?? '<none>'}`
)

console.log(
  `PASS: ${routes.length} localized documentation routes, ${searches.length} search queries, ${markdownExports.length} Markdown exports`
)
