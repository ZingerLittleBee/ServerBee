import archivoLatin from '@fontsource-variable/archivo/files/archivo-latin-wdth-normal.woff2?url'
import { createFileRoute, useParams } from '@tanstack/react-router'
import type { ComponentProps } from 'react'

import { LandingPage } from '@/components/landing'
import { type LandingSeoCopy, landingSeo } from '@/components/landing/seo'
import type { LandingLang } from '@/components/landing/translations'
import { languageLinks, ogLocale, SITE, shareImageAlt } from '@/lib/site'

/**
 * Latin Archivo sets the hero heading, subtitle and buttons. The stylesheet's @font-face uses this same URL, so the
 * preload starts the download before the stylesheet is parsed instead of fetching the file a second time.
 */
const fontPreloads: ComponentProps<'link'>[] = [
  { rel: 'preload', href: archivoLatin, as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' }
]

function toLandingLang(lang: string): LandingLang {
  return lang === 'zh' ? 'zh' : 'en'
}

export const Route = createFileRoute('/$lang/')({
  // The loader shares the page's lazy chunk, which keeps the landing copy (seo.ts imports it) out of the bundle that
  // every docs page loads. head only reads the loader's result.
  codeSplitGroupings: [['loader', 'component']],
  loader: ({ params }) => landingSeo[toLandingLang(params.lang)],
  head: ({ loaderData, params }) => landingHead(toLandingLang(params.lang), loaderData),
  component: Home
})

function landingHead(lang: LandingLang, seo: LandingSeoCopy | undefined) {
  if (!seo) {
    return { links: fontPreloads }
  }
  const url = `${SITE}/${lang}`
  const image = `${SITE}/og/landing-${lang}.png`
  const otherLang: LandingLang = lang === 'zh' ? 'en' : 'zh'
  return {
    meta: [
      { title: seo.title },
      { name: 'description', content: seo.description },
      { property: 'og:type', content: 'website' },
      { property: 'og:site_name', content: 'ServerBee' },
      { property: 'og:url', content: url },
      { property: 'og:title', content: seo.title },
      { property: 'og:description', content: seo.description },
      { property: 'og:locale', content: ogLocale[lang] },
      { property: 'og:locale:alternate', content: ogLocale[otherLang] },
      { property: 'og:image', content: image },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      { property: 'og:image:type', content: 'image/png' },
      { property: 'og:image:alt', content: shareImageAlt[lang] },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: seo.title },
      { name: 'twitter:description', content: seo.description },
      { name: 'twitter:image', content: image },
      { name: 'twitter:image:alt', content: shareImageAlt[lang] }
    ],
    links: [...languageLinks((code) => `${SITE}/${code}`, lang), ...fontPreloads]
  }
}

function Home() {
  const { lang } = useParams({ from: '/$lang/' })

  return <LandingPage lang={toLandingLang(lang)} />
}
