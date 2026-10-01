import { createRootRoute, HeadContent, Outlet, Scripts, useRouterState } from '@tanstack/react-router'
import { useSearchContext } from 'fumadocs-ui/contexts/search'
import { defineI18nUI } from 'fumadocs-ui/i18n'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'
import { lazy, useEffect } from 'react'

import { i18n } from '@/lib/i18n'
import appCss from '@/styles/app.css?url'

const SearchDialog = lazy(() => import('@/components/search-dialog'))

const { provider } = defineI18nUI(i18n, {
  translations: {
    en: {
      displayName: 'English'
    },
    zh: {
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
  // The landing (/en, /zh) has no search box, only the Cmd/Ctrl+K hotkey, so it loads the search dialog on first open
  // instead of preloading it.
  const isLanding = useRouterState({ select: (s) => s.matches.some((match) => match.routeId === '/$lang/') })
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
        <RootProvider i18n={provider(lang)} search={isLanding ? { SearchDialog, preload: false } : { SearchDialog }}>
          {isLanding ? null : <PreloadSearchDialog />}
          <Outlet />
        </RootProvider>
        <Scripts />
      </body>
    </html>
  )
}

/**
 * SearchProvider reads `preload` only on its first render, so a client-side move from the landing into the docs would
 * keep the dialog unloaded. Mounting it closed loads its chunk, as `preload` does when a docs page is opened directly.
 */
function PreloadSearchDialog() {
  const { open, setOpenSearch } = useSearchContext()

  useEffect(() => {
    if (!open) {
      setOpenSearch(false)
    }
  }, [open, setOpenSearch])

  return null
}
