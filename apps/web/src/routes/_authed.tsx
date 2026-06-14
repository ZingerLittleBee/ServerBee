import { createFileRoute, Link, Outlet, useLocation, useNavigate } from '@tanstack/react-router'
import { Fragment, useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AppSidebar } from '@/components/app-sidebar'
import { ThemeToggle } from '@/components/layout/theme-toggle'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator
} from '@/components/ui/breadcrumb'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Separator } from '@/components/ui/separator'
import { SidebarInset, SidebarProvider, SidebarTrigger } from '@/components/ui/sidebar'
import { ServersWsContext } from '@/contexts/servers-ws-context'
import { useAuth } from '@/hooks/use-auth'
import { useServersWs } from '@/hooks/use-servers-ws'
import { useWidgetModuleBootstrap } from '@/hooks/use-widget-module-bootstrap'
import type { ConnectionState } from '@/lib/ws-client'

const ROUTE_LABELS: Record<string, string> = {
  '/': 'nav_dashboard',
  '/servers': 'nav_servers',
  '/network': 'nav_network',
  '/traffic': 'nav_traffic',
  '/terminal': 'nav_terminal',
  '/files': 'nav_files',
  '/service-monitors': 'nav_service_monitors',
  '/security': 'nav_security_events',
  '/ip-quality': 'nav_ip_quality',
  '/settings': 'nav_settings',
  '/settings/users': 'nav_users',
  '/settings/notifications': 'nav_notifications',
  '/settings/alerts': 'nav_alerts',
  '/settings/ping-tasks': 'nav_ping_tasks',
  '/settings/service-monitors': 'nav_service_monitors',
  '/settings/status-pages': 'nav_status_pages',
  '/settings/network-probes': 'nav_network_probes',
  '/settings/firewall': 'nav_firewall',
  '/settings/ip-quality': 'nav_ip_quality_settings',
  '/settings/tasks': 'nav_commands',
  '/settings/capabilities': 'nav_capabilities',
  '/settings/api-keys': 'nav_api_keys',
  '/settings/mobile-devices': 'nav_mobile_devices',
  '/settings/rate-limits': 'nav_rate_limits',
  '/settings/security': 'nav_security',
  '/settings/appearance': 'nav_appearance',
  '/settings/widgets': 'nav_widgets',
  '/settings/audit-logs': 'nav_audit_logs'
}

interface BreadcrumbEntry {
  label: string
  to?: string
}

const TRAILING_SLASH_RE = /\/$/

function useBreadcrumbs(): BreadcrumbEntry[] {
  const { pathname } = useLocation()
  const { t } = useTranslation()

  return useMemo(() => {
    if (pathname === '/') {
      return [{ label: t('nav_dashboard') }]
    }

    const segments = pathname.replace(TRAILING_SLASH_RE, '').split('/').filter(Boolean)
    const crumbs: BreadcrumbEntry[] = []

    let accumulated = ''
    for (let i = 0; i < segments.length; i++) {
      accumulated += `/${segments[i]}`
      const labelKey = ROUTE_LABELS[accumulated]
      const isLast = i === segments.length - 1

      if (labelKey) {
        crumbs.push({
          label: t(labelKey),
          to: isLast ? undefined : accumulated
        })
      }
    }

    if (crumbs.length === 0) {
      const firstSegment = segments[0]
      const parentKey = ROUTE_LABELS[`/${firstSegment}`]
      if (parentKey) {
        crumbs.push({ label: t(parentKey), to: `/${firstSegment}` })
      }
    }

    return crumbs
  }, [pathname, t])
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

function LanguageSwitcher() {
  const { i18n } = useTranslation()
  const isZh = (i18n.resolvedLanguage ?? i18n.language).startsWith('zh')
  const toggle = () => i18n.changeLanguage(isZh ? 'en' : 'zh')

  return (
    <Button onClick={toggle} size="icon" variant="ghost">
      {isZh ? 'EN' : '中文'}
    </Button>
  )
}

export const Route = createFileRoute('/_authed')({
  component: AuthedLayout
})

function AuthedLayout() {
  const { isAuthenticated, isLoading, user } = useAuth()
  const { t } = useTranslation()
  const navigate = useNavigate()
  const breadcrumbs = useBreadcrumbs()
  const { pathname } = useLocation()
  const shouldConnectWs = isAuthenticated && !isLoading && user?.must_change_password !== true
  const wsRef = useServersWs(shouldConnectWs)
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected')

  useWidgetModuleBootstrap(shouldConnectWs)

  useEffect(() => {
    if (!shouldConnectWs) {
      setConnectionState('disconnected')
      return
    }

    const ws = wsRef.current
    if (!ws) {
      setConnectionState('disconnected')
      return
    }
    // Sync initial state
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
      <div className="flex min-h-screen items-center justify-center">
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
      <SidebarProvider>
        <AppSidebar />
        <SidebarInset className="min-h-0 overflow-hidden">
          <header className="flex h-14 shrink-0 items-center justify-between gap-2 px-3 sm:h-16 sm:px-4">
            <div className="flex min-w-0 items-center gap-2">
              <SidebarTrigger className="-ml-1" />
              <Separator
                className="mr-1 data-[orientation=vertical]:h-4 data-[orientation=vertical]:self-center sm:mr-2"
                orientation="vertical"
              />
              <Breadcrumb className="min-w-0">
                <BreadcrumbList className="min-w-0 flex-nowrap">
                  {breadcrumbs.map((crumb, index) => {
                    const isLast = index === breadcrumbs.length - 1
                    const hiddenOnMobile = index === 0 && breadcrumbs.length > 1
                    return (
                      <Fragment key={crumb.label}>
                        <BreadcrumbItem className={hiddenOnMobile ? 'hidden md:block' : 'min-w-0'}>
                          {isLast || !crumb.to ? (
                            <BreadcrumbPage className="truncate">{crumb.label}</BreadcrumbPage>
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
            </div>
            <div className="flex shrink-0 items-center gap-1 sm:gap-2">
              <LanguageSwitcher />
              <ThemeToggle />
            </div>
          </header>
          {showOffline && (
            <output className="flex shrink-0 items-center justify-center gap-2 bg-amber-500/15 px-3 py-1.5 text-amber-700 text-xs dark:text-amber-400">
              <span className="size-1.5 animate-pulse rounded-full bg-amber-500" />
              {t('connection_lost')}
            </output>
          )}
          <ScrollArea className="min-h-0 flex-1 overflow-hidden" contentClassName="min-w-0!">
            <main className="flex min-h-full min-w-0 flex-col p-3 pt-0 sm:p-4 sm:pt-0">
              <Outlet />
            </main>
          </ScrollArea>
        </SidebarInset>
      </SidebarProvider>
    </ServersWsContext.Provider>
  )
}
