import { useRouterState } from '@tanstack/react-router'
import { HomeLayout } from 'fumadocs-ui/layouts/home'
import { DefaultNotFound } from 'fumadocs-ui/layouts/home/not-found'

import { i18n } from '@/lib/i18n'
import { baseOptions } from '@/lib/layout.shared'

export function NotFound() {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const segment = pathname.split('/').filter(Boolean)[0] ?? ''
  const lang = (i18n.languages as string[]).includes(segment) ? segment : i18n.defaultLanguage

  return (
    <HomeLayout {...baseOptions(lang)}>
      <DefaultNotFound />
    </HomeLayout>
  )
}
