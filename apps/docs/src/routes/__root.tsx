import { createRootRoute, HeadContent, Outlet, Scripts, useRouterState } from '@tanstack/react-router'
import { defineI18nUI } from 'fumadocs-ui/i18n'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'
import { lazy, useEffect, useState } from 'react'

import { FrameworkLink } from '@/components/framework-link'
import { i18n, pathLanguage } from '@/lib/i18n'
import { uiTranslations } from '@/lib/ui-translations'
import appCss from '@/styles/app.css?url'

const SearchDialog = lazy(() => import('@/components/search-dialog'))

const applePlatform = /Mac|iPhone|iPad|iPod/

/** fumadocs shows ⌘ everywhere but Windows, so Linux and ChromeOS readers saw a key they do not have. */
function ModifierKey() {
  const [key, setKey] = useState('⌘')
  useEffect(() => {
    if (!applePlatform.test(navigator.userAgent)) {
      setKey('Ctrl')
    }
  }, [])
  return key
}

// fumadocs' default key matcher, so Cmd+K and Ctrl+K both open search everywhere. Defined once: SearchProvider
// subscribes its keydown listener again whenever this array changes.
const hotKey = [
  { key: (event: KeyboardEvent) => event.metaKey || event.ctrlKey, display: <ModifierKey /> },
  { key: 'k', display: 'K' }
]

const { provider } = defineI18nUI(i18n, { translations: uiTranslations })

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
  const lang = pathLanguage(pathname)

  return (
    <html lang={lang} suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex min-h-screen flex-col">
        <RootProvider components={{ Link: FrameworkLink }} i18n={provider(lang)} search={{ SearchDialog, hotKey }}>
          <Outlet />
        </RootProvider>
        <Scripts />
      </body>
    </html>
  )
}
