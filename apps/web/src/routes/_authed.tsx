import { createFileRoute, Link, Outlet, useLocation, useNavigate } from '@tanstack/react-router'
import { TriangleAlert } from 'lucide-react'
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
import type { ConnectionState } from '@/lib/ws-client'

const ROUTE_LABELS: Record<string, string> = {
  '/': 'nav_dashboard',
  '/servers': 'nav_servers',
  '/network': 'nav_network',
  '/traffic': 'nav_traffic',
  '/terminal': 'nav_terminal',
  '/files': 'nav_files',
  '/service-monitors': 'nav_service_monitors',
  '/settings': 'nav_settings',
  '/settings/users': 'nav_users',
  '/settings/notifications': 'nav_notifications',
  '/settings/alerts': 'nav_alerts',
  '/settings/ping-tasks': 'nav_ping_tasks',
  '/settings/service-monitors': 'nav_service_monitors',
  '/settings/status-pages': 'nav_status_pages',
  '/settings/network-probes': 'nav_network_probes',
  '/settings/tasks': 'nav_commands',
  '/settings/capabilities': 'nav_capabilities',
  '/settings/api-keys': 'nav_api_keys',
  '/settings/security': 'nav_security',
  '/settings/appearance': 'nav_appearance',
  '/settings/audit-logs': 'nav_audit_logs'
}

interface BreadcrumbEntry {
  label: string
  to?: string
}

function useBreadcrumbs(): BreadcrumbEntry[] {
  const { pathname } = useLocation()
  const { t } = useTranslation()

  return useMemo(() => {
    if (pathname === '/') {
      return [{ label: t('nav_dashboard') }]
    }

    const segments = pathname.replace(/\/$/, '').split('/').filter(Boolean)
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

const ADMIN_ONLY_ROUTES = [
  '/settings/notifications',
  '/settings/alerts',
  '/settings/tasks',
  '/settings/ping-tasks',
  '/settings/audit-logs',
  '/settings/users',
  '/settings/capabilities'
]

function isAdminRoute(pathname: string): boolean {
  if (pathname === '/settings' || pathname === '/settings/') {
    return true
  }
  return ADMIN_ONLY_ROUTES.some((route) => pathname === route || pathname.startsWith(`${route}/`))
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
  const shouldConnectWs = isAuthenticated && !isLoading
  const wsRef = useServersWs(shouldConnectWs)
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected')

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

  useEffect(() => {
    if (!(isLoading || isAuthenticated)) {
      navigate({ to: '/login' }).catch(() => {
        // Navigation error is non-critical
      })
    }
  }, [isLoading, isAuthenticated, navigate])

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

  if (user?.role !== 'admin' && isAdminRoute(pathname)) {
    return null
  }

  return (
    <ServersWsContext.Provider value={wsContextValue}>
      <SidebarProvider>
        <AppSidebar />
        <SidebarInset className="min-h-0 overflow-hidden">
          <header className="flex h-16 shrink-0 items-center justify-between gap-2 px-4">
            <div className="flex items-center gap-2">
              <SidebarTrigger className="-ml-1" />
              <Separator className="mr-2 self-center data-[orientation=vertical]:h-4" orientation="vertical" />
              <Breadcrumb>
                <BreadcrumbList>
                  {breadcrumbs.map((crumb, index) => {
                    const isLast = index === breadcrumbs.length - 1
                    const hiddenOnMobile = index === 0 && breadcrumbs.length > 1
                    return (
                      <Fragment key={crumb.label}>
                        <BreadcrumbItem className={hiddenOnMobile ? 'hidden md:block' : ''}>
                          {isLast || !crumb.to ? (
                            <BreadcrumbPage>{crumb.label}</BreadcrumbPage>
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
            <div className="flex items-center gap-2">
              <LanguageSwitcher />
              <ThemeToggle />
            </div>
          </header>
          {user?.must_change_password && <DefaultPasswordBanner />}
          <ScrollArea className="min-h-0 flex-1 overflow-hidden">
            <main className="p-4 pt-0">
              <Outlet />
            </main>
          </ScrollArea>
        </SidebarInset>
      </SidebarProvider>
    </ServersWsContext.Provider>
  )
}

function DefaultPasswordBanner() {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-2 border-amber-300 border-b bg-amber-50 px-6 py-2.5 text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
      <TriangleAlert className="size-4 shrink-0" />
      <p className="text-sm">
        {t('default_password_warning')}{' '}
        <Link className="font-medium underline underline-offset-2 hover:no-underline" to="/settings/security">
          {t('change_your_password')}
        </Link>{' '}
        {t('to_secure_account')}
      </p>
    </div>
  )
}
