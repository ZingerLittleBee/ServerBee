import { createRootRoute, HeadContent, Outlet, Scripts, useRouterState } from '@tanstack/react-router'
import { defineI18nUI } from 'fumadocs-ui/i18n'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'

import { i18n } from '@/lib/i18n'
import appCss from '@/styles/app.css?url'

const { provider } = defineI18nUI(i18n, {
  translations: {
    en: {
      displayName: 'English'
    },
    cn: {
      displayName: '中文',
      search: '搜索文档'
    }
  }
})

export const Route = createRootRoute({
  head: () => ({
    meta: [
      {
        charSet: 'utf-8'
      },
      {
        name: 'viewport',
        content: 'width=device-width, initial-scale=1'
      },
      {
        title: 'ServerBee Docs'
      }
    ],
    links: [
      { rel: 'stylesheet', href: appCss },
      { rel: 'icon', href: '/favicon.ico' }
    ]
  }),
  component: RootComponent
})

function RootComponent() {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const segment = pathname.split('/').filter(Boolean)[0] ?? ''
  const lang = (i18n.languages as string[]).includes(segment)
    ? (segment as (typeof i18n.languages)[number])
    : i18n.defaultLanguage

  return (
    <html lang={lang} suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex min-h-screen flex-col">
        <RootProvider i18n={provider(lang)}>
          <Outlet />
        </RootProvider>
        <Scripts />
      </body>
    </html>
  )
}
