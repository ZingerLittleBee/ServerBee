import { createFileRoute, Link, Outlet, useLocation, useNavigate } from '@tanstack/react-router'
import type { CSSProperties } from 'react'
import { Fragment, useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AppSidebar } from '@/components/app-sidebar'
import { SiteHeader } from '@/components/site-header'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator
} from '@/components/ui/breadcrumb'
import { ScrollArea } from '@/components/ui/scroll-area'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { ServersWsContext } from '@/contexts/servers-ws-context'
import { useAuth } from '@/hooks/use-auth'
import { useServersWs } from '@/hooks/use-servers-ws'
import { useWidgetModuleBootstrap } from '@/hooks/use-widget-module-bootstrap'
import { useServerDetail } from '@/lib/server-catalog'
import type { ConnectionState } from '@/lib/ws-client'
import { type BreadcrumbEntry, buildBreadcrumbs, getServerDetailId } from './_authed/components/breadcrumbs'

function useBreadcrumbs(enabled: boolean): BreadcrumbEntry[] {
  const { pathname } = useLocation()
  const { t } = useTranslation()
  const serverDetailId = getServerDetailId(pathname)
  const { data: server } = useServerDetail(serverDetailId, { enabled })

  return useMemo(() => buildBreadcrumbs(pathname, t, server?.name), [pathname, server?.name, t])
}

// Fail-closed admin gating: every route under /settings (and the /settings index)
// is admin-only EXCEPT the self-service pages members manage for themselves. A new
// settings page is admin-only by default unless explicitly added here. Keep this in
// sync with the `adminOnly` flags in app-sidebar.tsx — entries listed here must be
// the ones NOT marked adminOnly there.
const MEMBER_SETTINGS_ROUTES = ['/settings/mobile-devices', '/settings/api-keys', '/settings/security']

function isAdminRoute(pathname: string): boolean {
  if (!pathname.startsWith('/settings')) {
    return false
  }
  return !MEMBER_SETTINGS_ROUTES.some((route) => pathname === route || pathname.startsWith(`${route}/`))
}

export const Route = createFileRoute('/_authed')({
  component: AuthedLayout
})

function AuthedLayout() {
  const { isAuthenticated, isLoading, user } = useAuth()
  const { t } = useTranslation()
  const navigate = useNavigate()
  const breadcrumbs = useBreadcrumbs(isAuthenticated && !isLoading && user?.must_change_password !== true)
  const { pathname } = useLocation()
  const shouldConnectWs = isAuthenticated && !isLoading && user?.must_change_password !== true
  const wsRef = useServersWs(shouldConnectWs)
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected')

  useWidgetModuleBootstrap(shouldConnectWs)

  useEffect(() => {
    const ws = shouldConnectWs ? wsRef.current : null
    if (!ws) {
      setConnectionState('disconnected')
      return
    }

    setConnectionState(ws.connectionState)
    return ws.onConnectionStateChange(setConnectionState)
  }, [shouldConnectWs, wsRef])

  const send = useCallback(
    (data: unknown) => {
      wsRef.current?.send(data)
    },
    [wsRef]
  )

  const wsContextValue = useMemo(() => ({ send, connectionState }), [send, connectionState])

  // Surface a persistent disconnect to the user. Delay showing it so the initial
  // connect handshake and brief blips don't flash a banner; clear immediately on
  // reconnect.
  const [showOffline, setShowOffline] = useState(false)
  useEffect(() => {
    if (!shouldConnectWs || connectionState === 'connected') {
      setShowOffline(false)
      return
    }
    const timer = setTimeout(() => setShowOffline(true), 3000)
    return () => clearTimeout(timer)
  }, [shouldConnectWs, connectionState])

  useEffect(() => {
    if (!(isLoading || isAuthenticated)) {
      navigate({ to: '/login' }).catch(() => {
        // Navigation error is non-critical
      })
    }
  }, [isLoading, isAuthenticated, navigate])

  useEffect(() => {
    if (!isLoading && isAuthenticated && user?.must_change_password === true) {
      navigate({ to: '/onboarding' }).catch(() => {
        // Navigation error is non-critical
      })
    }
  }, [isLoading, isAuthenticated, user, navigate])

  useEffect(() => {
    if (!isLoading && isAuthenticated && user?.role !== 'admin' && isAdminRoute(pathname)) {
      navigate({ to: '/' }).catch(() => {
        // Navigation error is non-critical
      })
    }
  }, [isLoading, isAuthenticated, user, pathname, navigate])

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="space-y-4 text-center">
          <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
          <p className="text-muted-foreground text-sm">{t('loading')}</p>
        </div>
      </div>
    )
  }

  if (!isAuthenticated) {
    return null
  }

  if (user?.must_change_password === true) {
    return null
  }

  if (user?.role !== 'admin' && isAdminRoute(pathname)) {
    return null
  }

  return (
    <ServersWsContext.Provider value={wsContextValue}>
      <a
        className="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-background focus:px-3 focus:py-2 focus:text-sm focus:ring-2 focus:ring-ring"
        href="#main-content"
      >
        {t('a11y.skip_to_content')}
      </a>
      <SidebarProvider style={{ '--header-height': 'calc(var(--spacing) * 12)' } as CSSProperties}>
        <AppSidebar />
        <SidebarInset className="min-h-0 overflow-hidden">
          <SiteHeader>
            {breadcrumbs.length === 1 ? (
              <h1 className="truncate font-medium text-base">{breadcrumbs[0]?.label}</h1>
            ) : (
              <Breadcrumb className="min-w-0">
                <BreadcrumbList className="min-w-0 flex-nowrap">
                  {breadcrumbs.map((crumb, index) => {
                    const isLast = index === breadcrumbs.length - 1
                    const hiddenOnMobile = index === 0 && breadcrumbs.length > 1
                    return (
                      <Fragment key={crumb.label}>
                        <BreadcrumbItem className={hiddenOnMobile ? 'hidden md:block' : 'min-w-0'}>
                          {isLast || !crumb.to ? (
                            <BreadcrumbPage className="truncate font-medium text-base">{crumb.label}</BreadcrumbPage>
                          ) : (
                            <BreadcrumbLink render={<Link to={crumb.to} />}>{crumb.label}</BreadcrumbLink>
                          )}
                        </BreadcrumbItem>
                        {!isLast && <BreadcrumbSeparator className={hiddenOnMobile ? 'hidden md:block' : ''} />}
                      </Fragment>
                    )
                  })}
                </BreadcrumbList>
              </Breadcrumb>
            )}
          </SiteHeader>
          {showOffline && (
            <output className="flex shrink-0 items-center justify-center gap-2 bg-amber-500/15 px-3 py-1.5 text-amber-700 text-xs dark:text-amber-400">
              <span className="size-1.5 animate-pulse rounded-full bg-amber-500" />
              {t('connection_lost')}
            </output>
          )}
          <ScrollArea className="min-h-0 flex-1 overflow-hidden" contentClassName="min-w-0!">
            {/* SidebarInset already renders the page's <main> landmark; this is only
                the skip-link target. */}
            <div className="flex min-h-full min-w-0 flex-col" id="main-content" tabIndex={-1}>
              <Outlet />
            </div>
          </ScrollArea>
        </SidebarInset>
      </SidebarProvider>
    </ServersWsContext.Provider>
  )
}
