import { createFileRoute, useParams } from '@tanstack/react-router'

import { LandingPage } from '@/components/landing'
import { type LandingSeoCopy, landingSeo } from '@/components/landing/seo'
import type { LandingLang } from '@/components/landing/translations'

/** The canonical docs host. /en/ redirects to /en, so page URLs carry no trailing slash. */
const SITE = 'https://docs.serverbee.app'

const ogLocale: Record<LandingLang, string> = { en: 'en_US', zh: 'zh_CN' }

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
    return {}
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
      { property: 'og:image:alt', content: seo.imageAlt },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: seo.title },
      { name: 'twitter:description', content: seo.description },
      { name: 'twitter:image', content: image },
      { name: 'twitter:image:alt', content: seo.imageAlt }
    ],
    links: [
      { rel: 'canonical', href: url },
      { rel: 'alternate', hrefLang: 'en', href: `${SITE}/en` },
      { rel: 'alternate', hrefLang: 'zh-Hans', href: `${SITE}/zh` },
      { rel: 'alternate', hrefLang: 'x-default', href: `${SITE}/en` }
    ]
  }
}

function Home() {
  const { lang } = useParams({ from: '/$lang/' })

  return <LandingPage lang={toLandingLang(lang)} />
}
