import { Link, useRouterState } from '@tanstack/react-router'
import { buttonVariants } from 'fumadocs-ui/components/ui/button'
import { HomeLayout } from 'fumadocs-ui/layouts/home'
import { BookOpen } from 'lucide-react'

import { type DocsLanguage, pathLanguage } from '@/lib/i18n'
import { baseOptions } from '@/lib/layout.shared'
import { notFoundTitle } from '@/lib/site'

const copy: Record<DocsLanguage, { body: string; docs: string }> = {
  en: {
    body: 'This page may have been moved or removed. Search the docs, or start from the introduction.',
    docs: 'Open the docs'
  },
  zh: {
    body: '这个页面可能已经移动或删除。可以搜索文档，或者从介绍页开始。',
    docs: '打开文档'
  }
}

/**
 * fumadocs' DefaultNotFound is English only and links to /, which redirects to the English docs. This one follows the
 * language of the requested URL and links to that language's docs. The link matches exactly, so it is not announced as
 * the current page.
 */
export function NotFound() {
  const lang = useRouterState({ select: (s) => pathLanguage(s.location.pathname) })
  const text = copy[lang]

  return (
    <HomeLayout {...baseOptions(lang)}>
      <div className="flex flex-1 flex-col items-center justify-center gap-4 px-8 text-center">
        <p className="font-bold text-6xl text-fd-muted-foreground">404</p>
        <h1 className="font-semibold text-2xl">{notFoundTitle[lang]}</h1>
        <p className="max-w-md text-balance break-keep text-fd-muted-foreground">{text.body}</p>
        <Link
          activeOptions={{ exact: true }}
          className={buttonVariants({ className: 'mt-4 gap-1.5', variant: 'primary' })}
          params={{ lang, _splat: '' }}
          to="/$lang/docs/$"
        >
          <BookOpen className="size-4" />
          {text.docs}
        </Link>
      </div>
    </HomeLayout>
  )
}
