import { createFileRoute, Link, useParams } from '@tanstack/react-router'
import { HomeLayout } from 'fumadocs-ui/layouts/home'

import { baseOptions } from '@/lib/layout.shared'

export const Route = createFileRoute('/$lang/')({
  component: Home
})

const texts = {
  en: { heading: 'ServerBee Documentation', cta: 'Open Docs' },
  cn: { heading: 'ServerBee 文档', cta: '打开文档' }
} as const

function Home() {
  const { lang } = useParams({ from: '/$lang/' })
  const t = texts[lang as keyof typeof texts] ?? texts.en

  return (
    <HomeLayout {...baseOptions(lang)}>
      <div className="flex flex-1 flex-col justify-center px-4 py-8 text-center">
        <h1 className="mb-4 font-medium text-xl">{t.heading}</h1>
        <Link
          className="mx-auto rounded-lg bg-fd-primary px-3 py-2 font-medium text-fd-primary-foreground text-sm"
          params={{ lang, _splat: '' }}
          to="/$lang/docs/$"
        >
          {t.cta}
        </Link>
      </div>
    </HomeLayout>
  )
}
