import { Link, useMatchRoute } from '@tanstack/react-router'
import {
  Activity,
  AlertTriangle,
  Bell,
  ClipboardList,
  Key,
  LayoutDashboard,
  List,
  Server,
  Settings,
  Shield,
  Terminal,
  Users
} from 'lucide-react'
import { useAuth } from '@/hooks/use-auth'
import { cn } from '@/lib/utils'

const navItems = [
  { to: '/', label: 'Dashboard', icon: LayoutDashboard },
  { to: '/servers', label: 'Servers', icon: List },
  { to: '/settings/users', label: 'Users', icon: Users, adminOnly: true },
  { to: '/settings/notifications', label: 'Notifications', icon: Bell, adminOnly: true },
  { to: '/settings/alerts', label: 'Alerts', icon: AlertTriangle, adminOnly: true },
  { to: '/settings/ping-tasks', label: 'Ping Tasks', icon: Activity },
  { to: '/settings/tasks', label: 'Commands', icon: Terminal, adminOnly: true },
  { to: '/settings/capabilities', label: 'Capabilities', icon: Shield, adminOnly: true },
  { to: '/settings/api-keys', label: 'API Keys', icon: Key },
  { to: '/settings/security', label: 'Security', icon: Shield },
  { to: '/settings/audit-logs', label: 'Audit Logs', icon: ClipboardList, adminOnly: true },
  { to: '/settings', label: 'Settings', icon: Settings, adminOnly: true }
] as const

export function Sidebar() {
  const matchRoute = useMatchRoute()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-14 items-center gap-2 border-b px-4">
        <Server className="size-5 text-sidebar-primary" />
        <span className="font-semibold text-sm">ServerBee</span>
      </div>
      <nav className="flex-1 space-y-1 p-2">
        {navItems.map((item) => {
          if ('adminOnly' in item && item.adminOnly && !isAdmin) {
            return null
          }
          const isActive = matchRoute({ to: item.to, fuzzy: true })
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
              <item.icon className="size-4" />
              {item.label}
            </Link>
          )
        })}
      </nav>
    </aside>
  )
}
