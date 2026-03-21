import { Link, useMatchRoute } from '@tanstack/react-router'
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Bell,
  ClipboardList,
  Globe,
  HeartPulse,
  Key,
  LayoutDashboard,
  List,
  Monitor,
  Palette,
  Radar,
  Settings,
  Shield,
  Terminal,
  Users,
  Wifi
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { useAuth } from '@/hooks/use-auth'
import { cn } from '@/lib/utils'

const navItems = [
  { to: '/', labelKey: 'nav_dashboard' as const, icon: LayoutDashboard },
  { to: '/servers', labelKey: 'nav_servers' as const, icon: List },
  { to: '/network', labelKey: 'nav_network' as const, icon: Wifi },
  { to: '/traffic', labelKey: 'nav_traffic' as const, icon: BarChart3 },
  { to: '/settings/users', labelKey: 'nav_users' as const, icon: Users, adminOnly: true },
  { to: '/settings/notifications', labelKey: 'nav_notifications' as const, icon: Bell, adminOnly: true },
  { to: '/settings/alerts', labelKey: 'nav_alerts' as const, icon: AlertTriangle, adminOnly: true },
  { to: '/settings/ping-tasks', labelKey: 'nav_ping_tasks' as const, icon: Activity },
  { to: '/settings/service-monitors', labelKey: 'nav_service_monitors' as const, icon: HeartPulse },
  { to: '/settings/status-pages', labelKey: 'nav_status_pages' as const, icon: Monitor, adminOnly: true },
  { to: '/settings/network-probes', labelKey: 'nav_network_probes' as const, icon: Globe, adminOnly: true },
  { to: '/settings/tasks', labelKey: 'nav_commands' as const, icon: Terminal, adminOnly: true },
  { to: '/settings/capabilities', labelKey: 'nav_capabilities' as const, icon: Shield, adminOnly: true },
  { to: '/settings/api-keys', labelKey: 'nav_api_keys' as const, icon: Key },
  { to: '/settings/security', labelKey: 'nav_security' as const, icon: Shield },
  { to: '/settings/appearance', labelKey: 'nav_appearance' as const, icon: Palette },
  { to: '/settings/audit-logs', labelKey: 'nav_audit_logs' as const, icon: ClipboardList, adminOnly: true },
  { to: '/settings', labelKey: 'nav_settings' as const, icon: Settings, adminOnly: true }
] as const

function SidebarContent({ onNavigate }: { onNavigate?: () => void }) {
  const matchRoute = useMatchRoute()
  const { user } = useAuth()
  const { t } = useTranslation()
  const isAdmin = user?.role === 'admin'

  return (
    <>
      <div className="flex h-14 items-center gap-2 border-b px-4">
        <Radar aria-hidden="true" className="size-5 text-primary-background" />
        <span className="font-semibold text-sm">ServerBee</span>
      </div>
      <nav aria-label={t('common:main_navigation')} className="flex-1 space-y-1 overflow-y-auto p-2">
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
              onClick={onNavigate}
              to={item.to}
            >
              <item.icon aria-hidden="true" className="size-4" />
              {t(item.labelKey)}
            </Link>
          )
        })}
      </nav>
    </>
  )
}

export function Sidebar() {
  const { t } = useTranslation()

  return (
    <aside
      aria-label={t('common:main_navigation')}
      className="hidden w-56 shrink-0 flex-col border-r bg-sidebar lg:flex"
    >
      <SidebarContent />
    </aside>
  )
}

export function MobileSidebar({ onOpenChange, open }: { onOpenChange: (open: boolean) => void; open: boolean }) {
  const { t } = useTranslation()

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent className="w-64 bg-sidebar p-0" side="left">
        <SheetHeader className="sr-only">
          <SheetTitle>{t('common:main_navigation')}</SheetTitle>
          <SheetDescription>{t('common:main_navigation')}</SheetDescription>
        </SheetHeader>
        <SidebarContent onNavigate={() => onOpenChange(false)} />
      </SheetContent>
    </Sheet>
  )
}
