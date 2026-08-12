import { setTimeout as delay } from 'node:timers/promises'

const baseUrl = process.env.SERVERBEE_DOCS_BASE_URL ?? 'http://127.0.0.1:4000'

const routes = [
  { path: '/en', lang: 'en', marker: 'Self-hosted VPS monitoring' },
  { path: '/zh', lang: 'zh', marker: '自托管的 VPS 监控' },
  { path: '/en/docs/quick-start', lang: 'en', marker: 'Choose a deployment method' },
  { path: '/zh/docs/quick-start', lang: 'zh', marker: '先选择部署方式' },
  { path: '/en/docs/configuration', lang: 'en', marker: 'Configuration Loading Priority' },
  { path: '/zh/docs/configuration', lang: 'zh', marker: '配置加载优先级' }
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
}

console.log(`PASS: ${routes.length} localized documentation routes`)
