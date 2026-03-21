import { createFileRoute, Link, Outlet, useLocation, useNavigate } from '@tanstack/react-router'
import { TriangleAlert } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Header } from '@/components/layout/header'
import { MobileSidebar, Sidebar } from '@/components/layout/sidebar'
import { ServersWsContext } from '@/contexts/servers-ws-context'
import { useAuth } from '@/hooks/use-auth'
import { useServersWs } from '@/hooks/use-servers-ws'
import type { ConnectionState } from '@/lib/ws-client'

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

export const Route = createFileRoute('/_authed')({
  component: AuthedLayout
})

function AuthedLayout() {
  const { isAuthenticated, isLoading, user } = useAuth()
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const shouldConnectWs = isAuthenticated && !isLoading
  const wsRef = useServersWs(shouldConnectWs)
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected')
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)

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
      <div className="flex min-h-screen">
        <Sidebar />
        <MobileSidebar onOpenChange={setMobileMenuOpen} open={mobileMenuOpen} />
        <div className="flex min-w-0 flex-1 flex-col">
          <Header onMenuClick={() => setMobileMenuOpen(true)} />
          {user?.must_change_password && <DefaultPasswordBanner />}
          <main className="flex-1 overflow-auto p-4 sm:p-6">
            <Outlet />
          </main>
        </div>
      </div>
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
