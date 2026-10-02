import { type Browser, chromium, firefox, type Page, webkit } from 'playwright'

const baseUrl = process.env.BASE_URL ?? 'http://127.0.0.1:4000'
const engines = { chromium, firefox, webkit }
const requested = (process.env.BROWSERS ?? 'chromium').split(',')
const tolerance = 3

function check(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(message)
  }
}

async function waitHydrated(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    const root = document.querySelector('.sb') ?? document.querySelector('#nd-docs-layout')
    return (
      document.readyState === 'complete' && root !== null && Object.keys(root).some((key) => key.startsWith('__react'))
    )
  })
  await page.evaluate(() => document.fonts.ready.then(() => undefined))
}

async function go(page: Page, path: string): Promise<void> {
  const response = await page.goto(new URL(path, baseUrl).href)
  check(response?.status() === 200, `GET ${path} returned ${response?.status()}`)
  await waitHydrated(page)
}

async function scroll(page: Page, top: number): Promise<number> {
  await page.evaluate((position) => window.scrollTo({ top: position, behavior: 'instant' }), top)
  // The router saves scroll positions with a throttle; wait for several rendering frames after the scroll event.
  await page.waitForTimeout(200)
  return page.evaluate(() => Math.round(window.scrollY))
}

async function expectPosition(page: Page, expected: number, label: string): Promise<void> {
  await page.waitForFunction(({ position, margin }) => Math.abs(window.scrollY - position) <= margin, {
    position: expected,
    margin: tolerance
  })
  // A restoration must stay put after subsequent layout, rather than merely passing through the saved position.
  await page.waitForTimeout(250)
  const actual = await page.evaluate(() => Math.round(window.scrollY))
  check(Math.abs(actual - expected) <= tolerance, `${label}: expected y=${expected}, got y=${actual}`)
}

async function follow(page: Page, selector: string, path: string): Promise<void> {
  await page.locator(selector).first().click()
  await page.waitForURL(new URL(path, baseUrl).href)
  await waitHydrated(page)
}

async function back(page: Page, path: string, position: number): Promise<void> {
  await page.goBack()
  await page.waitForURL(new URL(path, baseUrl).href)
  await waitHydrated(page)
  await expectPosition(page, position, `Back to ${path}`)
}

async function forward(page: Page, path: string, position: number): Promise<void> {
  await page.goForward()
  await page.waitForURL(new URL(path, baseUrl).href)
  await waitHydrated(page)
  await expectPosition(page, position, `Forward to ${path}`)
}

interface Scenario {
  name: string
  run: (page: Page) => Promise<void>
}

function beforeHydration(path: string): Scenario {
  return {
    name: `preserve scrolling before hydration: ${path}`,
    async run(page) {
      let releaseScripts: () => void = () => undefined
      const scriptsReady = new Promise<void>((resolve) => {
        releaseScripts = resolve
      })
      await page.route('**/*', async (route) => {
        if (route.request().resourceType() === 'script') {
          await scriptsReady
        }
        await route.continue()
      })
      try {
        const response = await page.goto(new URL(path, baseUrl).href, { waitUntil: 'commit' })
        check(response?.status() === 200, `GET ${path} returned ${response?.status()}`)
        await page.locator(path === '/en' ? '.sb' : '#nd-docs-layout').waitFor()
        await page.evaluate(() => document.fonts.ready.then(() => undefined))
        await page.mouse.move(700, 450)
        await page.mouse.wheel(0, 1400)
        await page.waitForFunction(() => window.scrollY > 500)
        await page.waitForTimeout(200)
        const before = await page.evaluate(() => Math.round(window.scrollY))
        releaseScripts()
        await waitHydrated(page)
        await expectPosition(page, before, 'Hydration')
      } finally {
        releaseScripts()
        await page.unrouteAll({ behavior: 'wait' })
      }
    }
  }
}

async function leaveLanding(page: Page): Promise<void> {
  await follow(page, '.sb .nav a[href="/en/docs/quick-start"]', '/en/docs/quick-start')
  await expectPosition(page, 0, 'New docs page')
}

function fragmentHistory(name: string, source: string, selector: string, target: string): Scenario {
  return {
    name,
    async run(page) {
      await go(page, source)
      await follow(page, selector, target)
      const jumped = await page.evaluate(() => Math.round(window.scrollY))
      check(jumped > 0, `The fragment in ${target} must scroll to its target`)
      const reader = await scroll(page, Math.max(500, jumped - 600))
      check(Math.abs(reader - jumped) > 100, 'Scroll away from the fragment before leaving')
      if (source === '/en') {
        await leaveLanding(page)
      } else {
        await follow(page, 'aside a[href="/en/docs/configuration"]', '/en/docs/configuration')
      }
      const destination = new URL(page.url()).pathname
      await back(page, target, reader)
      await forward(page, destination, 0)
      await back(page, target, reader)
    }
  }
}

const scenarios: Scenario[] = [
  {
    name: 'fresh landing and docs start at the top',
    async run(page) {
      await go(page, '/en')
      await expectPosition(page, 0, 'Fresh landing')
      await leaveLanding(page)
    }
  },
  beforeHydration('/en'),
  beforeHydration('/en/docs/quick-start'),
  beforeHydration('/zh/docs/quick-start'),
  {
    name: 'Back and Forward restore a long docs page',
    async run(page) {
      await go(page, '/en/docs/configuration')
      const reader = await scroll(page, 6000)
      check(reader > 3000, 'The configuration page must be long enough to exercise restoration')
      await follow(page, 'aside a[href="/en/docs/quick-start"]', '/en/docs/quick-start')
      await expectPosition(page, 0, 'New docs page')
      await back(page, '/en/docs/configuration', reader)
      await forward(page, '/en/docs/quick-start', 0)
    }
  },
  ...['/en', '/en#faq', '/en/docs/quick-start'].map(
    (path): Scenario => ({
      name: `reload keeps the reader's position: ${path}`,
      async run(page) {
        await go(page, path)
        const reader = await scroll(page, 1200)
        await page.reload()
        await waitHydrated(page)
        await expectPosition(page, reader, 'Reload')
      }
    })
  ),
  fragmentHistory('landing anchor survives Back and Forward', '/en', '.nav-links a[href="#ipq"]', '/en#ipq'),
  fragmentHistory('landing note survives Back and Forward', '/en', 'a[href="#how-note-2"]', '/en#how-note-2'),
  fragmentHistory(
    'docs table of contents survives Back and Forward',
    '/en/docs/quick-start',
    '#nd-toc a[href="#manage-the-agent"]',
    '/en/docs/quick-start#manage-the-agent'
  )
]

async function runScenarios(browser: Browser, engine: string): Promise<number> {
  let failures = 0
  for (const scenario of scenarios) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' })
    const page = await context.newPage()
    page.setDefaultTimeout(10_000)
    page.setDefaultNavigationTimeout(15_000)
    const errors: string[] = []
    page.on('pageerror', (error) => errors.push(error.message))
    try {
      await scenario.run(page)
      check(errors.length === 0, `Uncaught browser errors: ${errors.join('; ')}`)
      console.log(`PASS [${engine}] ${scenario.name}`)
    } catch (error) {
      failures += 1
      console.error(`FAIL [${engine}] ${scenario.name}`, error)
    } finally {
      await context.close()
    }
  }
  return failures
}

let failures = 0
function isEngineName(name: string): name is keyof typeof engines {
  return Object.hasOwn(engines, name)
}

for (const name of requested) {
  if (!isEngineName(name)) {
    throw new Error(`Unknown browser '${name}'; use BROWSERS=chromium,firefox,webkit`)
  }
  const engine = engines[name]
  const browser = await engine.launch()
  try {
    failures += await runScenarios(browser, name)
  } finally {
    await browser.close()
  }
}
console.log(`${scenarios.length * requested.length} browser scenarios executed, ${failures} failed at ${baseUrl}`)
process.exitCode = failures === 0 ? 0 : 1
