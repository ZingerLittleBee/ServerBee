/*
 * Renders the link preview images the landing page names in its og:image and twitter:image tags.
 *
 *   bun run dev             # terminal 1, in apps/landing (or `bun run build && bun run preview`)
 *   bun run capture:og      # terminal 2
 *
 * A card is laid out inside the running page, so it uses the page's own fonts, colors, logo, headline and fleet
 * capture. Run it again after `bun run capture`, or after the headline changes.
 *
 * Output: public/og/landing-{lang}.png, 1200 by 630.
 */

import { mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'
import { homePath, LANGS } from '../src/lib/site.ts'

const BASE = process.env.LANDING_URL ?? 'http://localhost:4100'
const OUT = join(resolve(dirname(fileURLToPath(import.meta.url)), '..'), 'public/og')
const SIZE = { width: 1200, height: 630 }

const CARD_CSS = `
  html { color-scheme: light; }
  body { width: ${SIZE.width}px; height: ${SIZE.height}px; overflow: hidden; background: var(--bg); }
  .og { position: relative; height: 100%; padding: 56px 64px 0; }
  .og .logo { font-size: 28px; gap: 12px; }
  .og .logo svg { width: 32px; height: 32px; }
  .og h1 { margin-top: 30px; font-size: 80px; }
  .og h1 span { display: block; }
  .og:lang(zh) h1 { font-size: 72px; line-height: 1.12; }
  .og figure {
    position: absolute; top: 326px; left: 64px; width: 1320px; margin: 0; overflow: hidden;
    border: 1px solid var(--line-strong); border-radius: 14px; box-shadow: var(--shot-shadow);
  }
  .og figure img { display: block; width: 100%; height: auto; }
`

async function main() {
  mkdirSync(OUT, { recursive: true })
  const browser = await chromium.launch()
  try {
    for (const lang of LANGS) {
      const page = await browser.newPage({ viewport: SIZE, colorScheme: 'light', deviceScaleFactor: 1 })
      // A wide first load makes the page pick the full-size fleet capture.
      await page.setViewportSize({ width: 1440, height: 900 })
      await page.goto(`${BASE}${homePath(lang)}`, { waitUntil: 'networkidle' })
      await page.evaluate(async (css) => {
        const logo = document.querySelector('.logo')?.outerHTML
        const heading = document.querySelector('.hero h1')?.cloneNode(true)
        const shot = document.querySelector<HTMLImageElement>('.hero-shot img')?.currentSrc
        if (!(logo && heading && shot)) {
          throw new Error('capture-og: the page has no logo, no headline or no hero capture')
        }
        document.head.append(Object.assign(document.createElement('style'), { textContent: css }))
        const card = Object.assign(document.createElement('div'), { className: 'og', innerHTML: logo })
        const figure = document.createElement('figure')
        const image = Object.assign(new Image(), { src: shot })
        figure.append(image)
        card.append(heading, figure)
        document.body.replaceChildren(card)
        await image.decode()
        await document.fonts.ready
      }, CARD_CSS)
      await page.setViewportSize(SIZE)
      await page.screenshot({ path: join(OUT, `landing-${lang}.png`), type: 'png' })
      await page.close()
      console.log(`og landing-${lang}`)
    }
  } finally {
    await browser.close()
  }
}

await main()
