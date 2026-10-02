import { createRootRoute, HeadContent, Outlet, Scripts, useRouterState } from '@tanstack/react-router'
import { useSearchContext } from 'fumadocs-ui/contexts/search'
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
  // The landing (/en, /zh) has no search box, only the Cmd/Ctrl+K hotkey, so it loads the search dialog once the page
  // is idle instead of with the page.
  const isLanding = useRouterState({ select: (s) => s.matches.some((match) => match.routeId === '/$lang/') })
  const lang = pathLanguage(pathname)

  return (
    <html lang={lang} suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex min-h-screen flex-col">
        <RootProvider
          components={{ Link: FrameworkLink }}
          i18n={provider(lang)}
          search={isLanding ? { SearchDialog, hotKey, preload: false } : { SearchDialog, hotKey }}
        >
          <PreloadSearchDialog whenIdle={isLanding} />
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
 * The landing waits for an idle moment: a dialog first mounted by the hotkey suspends while its chunk loads (and React
 * holds a revealed boundary back for at least 300 ms), so the first characters typed after Cmd/Ctrl+K were lost.
 */
function PreloadSearchDialog({ whenIdle }: { whenIdle: boolean }) {
  const { open, setOpenSearch } = useSearchContext()

  useEffect(() => {
    if (open) {
      return
    }
    if (!whenIdle) {
      setOpenSearch(false)
      return
    }
    const mount = () => setOpenSearch(false)
    if (typeof requestIdleCallback === 'function') {
      const id = requestIdleCallback(mount, { timeout: 5000 })
      return () => cancelIdleCallback(id)
    }
    // Safari has no requestIdleCallback, so it mounts the dialog once the page has loaded. A fixed delay kept the
    // hotkey losing keys for as long as it lasted.
    if (document.readyState === 'complete') {
      mount()
      return
    }
    window.addEventListener('load', mount, { once: true })
    return () => window.removeEventListener('load', mount)
  }, [open, setOpenSearch, whenIdle])

  return null
}
