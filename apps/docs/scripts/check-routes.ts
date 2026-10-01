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

console.log(`PASS: ${routes.length} localized documentation routes, ${searches.length} search queries`)
