import { Link, useMatchRoute } from '@tanstack/react-router'
import {
  Activity,
  AlertTriangle,
  Bell,
  ClipboardList,
  Globe,
  HeartPulse,
  Key,
  LayoutDashboard,
  List,
  Radar,
  Settings,
  Shield,
  Terminal,
  Users,
  Wifi
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAuth } from '@/hooks/use-auth'
import { cn } from '@/lib/utils'

const navItems = [
  { to: '/', labelKey: 'nav_dashboard' as const, icon: LayoutDashboard },
  { to: '/servers', labelKey: 'nav_servers' as const, icon: List },
  { to: '/network', labelKey: 'nav_network' as const, icon: Wifi },
  { to: '/settings/users', labelKey: 'nav_users' as const, icon: Users, adminOnly: true },
  { to: '/settings/notifications', labelKey: 'nav_notifications' as const, icon: Bell, adminOnly: true },
  { to: '/settings/alerts', labelKey: 'nav_alerts' as const, icon: AlertTriangle, adminOnly: true },
  { to: '/settings/ping-tasks', labelKey: 'nav_ping_tasks' as const, icon: Activity },
  { to: '/settings/service-monitors', labelKey: 'nav_service_monitors' as const, icon: HeartPulse },
  { to: '/settings/network-probes', labelKey: 'nav_network_probes' as const, icon: Globe, adminOnly: true },
  { to: '/settings/tasks', labelKey: 'nav_commands' as const, icon: Terminal, adminOnly: true },
  { to: '/settings/capabilities', labelKey: 'nav_capabilities' as const, icon: Shield, adminOnly: true },
  { to: '/settings/api-keys', labelKey: 'nav_api_keys' as const, icon: Key },
  { to: '/settings/security', labelKey: 'nav_security' as const, icon: Shield },
  { to: '/settings/audit-logs', labelKey: 'nav_audit_logs' as const, icon: ClipboardList, adminOnly: true },
  { to: '/settings', labelKey: 'nav_settings' as const, icon: Settings, adminOnly: true }
] as const

export function Sidebar() {
  const matchRoute = useMatchRoute()
  const { user } = useAuth()
  const { t } = useTranslation()
  const isAdmin = user?.role === 'admin'

  return (
    <aside aria-label={t('common:main_navigation')} className="flex w-56 shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-14 items-center gap-2 border-b px-4">
        <Radar aria-hidden="true" className="size-5 text-primary-background" />
        <span className="font-semibold text-sm">ServerBee</span>
      </div>
      <nav aria-label={t('common:main_navigation')} className="flex-1 space-y-1 p-2">
        {navItems.map((item) => {
          if ('adminOnly' in item && item.adminOnly && !isAdmin) {
            return null
          }
          const isActive = matchRoute({ to: item.to, fuzzy: item.to === '/servers' })
          return (
            <Link
              className={cn(
                'flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors',
                isActive
                  ? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
                  : 'text-sidebar-foreground hover:bg-sidebar-accent/50'
              )}
              key={item.to}
              to={item.to}
            >
              <item.icon aria-hidden="true" className="size-4" />
              {t(item.labelKey)}
            </Link>
          )
        })}
      </nav>
    </aside>
  )
}
