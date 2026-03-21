import { Link, useMatchRoute, useNavigate } from '@tanstack/react-router'
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Bell,
  ChevronsUpDown,
  ClipboardList,
  Globe,
  HeartPulse,
  Key,
  LayoutDashboard,
  List,
  LogOut,
  Monitor,
  Palette,
  Radar,
  Settings,
  Shield,
  Terminal,
  Users,
  Wifi
} from 'lucide-react'
import type * as React from 'react'
import { useTranslation } from 'react-i18next'
import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar
} from '@/components/ui/sidebar'
import { useAuth } from '@/hooks/use-auth'

const monitorItems = [
  { to: '/', labelKey: 'nav_dashboard', icon: LayoutDashboard },
  { to: '/servers', labelKey: 'nav_servers', icon: List },
  { to: '/network', labelKey: 'nav_network', icon: Wifi },
  { to: '/traffic', labelKey: 'nav_traffic', icon: BarChart3 }
] as const

const settingsItems = [
  { to: '/settings/users', labelKey: 'nav_users', icon: Users, adminOnly: true },
  { to: '/settings/notifications', labelKey: 'nav_notifications', icon: Bell, adminOnly: true },
  { to: '/settings/alerts', labelKey: 'nav_alerts', icon: AlertTriangle, adminOnly: true },
  { to: '/settings/ping-tasks', labelKey: 'nav_ping_tasks', icon: Activity },
  { to: '/settings/service-monitors', labelKey: 'nav_service_monitors', icon: HeartPulse },
  { to: '/settings/status-pages', labelKey: 'nav_status_pages', icon: Monitor, adminOnly: true },
  { to: '/settings/network-probes', labelKey: 'nav_network_probes', icon: Globe, adminOnly: true },
  { to: '/settings/tasks', labelKey: 'nav_commands', icon: Terminal, adminOnly: true },
  { to: '/settings/capabilities', labelKey: 'nav_capabilities', icon: Shield, adminOnly: true },
  { to: '/settings/api-keys', labelKey: 'nav_api_keys', icon: Key },
  { to: '/settings/security', labelKey: 'nav_security', icon: Shield },
  { to: '/settings/appearance', labelKey: 'nav_appearance', icon: Palette },
  { to: '/settings/audit-logs', labelKey: 'nav_audit_logs', icon: ClipboardList, adminOnly: true },
  { to: '/settings', labelKey: 'nav_settings', icon: Settings, adminOnly: true }
] as const

export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  const { t } = useTranslation()
  const { user, logout } = useAuth()
  const matchRoute = useMatchRoute()
  const navigate = useNavigate()
  const { isMobile } = useSidebar()
  const isAdmin = user?.role === 'admin'

  const handleLogout = async () => {
    await logout()
    navigate({ to: '/login' })
  }

  return (
    <Sidebar variant="inset" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton render={<Link to="/" />} size="lg">
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                <Radar className="size-4" />
              </div>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-medium">ServerBee</span>
                <span className="truncate text-xs">Monitoring</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>{t('nav_dashboard')}</SidebarGroupLabel>
          <SidebarMenu>
            {monitorItems.map((item) => {
              const isActive = matchRoute({ to: item.to, fuzzy: item.to === '/servers' })
              return (
                <SidebarMenuItem key={item.to}>
                  <SidebarMenuButton isActive={!!isActive} render={<Link to={item.to} />} tooltip={t(item.labelKey)}>
                    <item.icon />
                    <span>{t(item.labelKey)}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )
            })}
          </SidebarMenu>
        </SidebarGroup>
        <SidebarGroup>
          <SidebarGroupLabel>{t('nav_settings')}</SidebarGroupLabel>
          <SidebarMenu>
            {settingsItems.map((item) => {
              if ('adminOnly' in item && item.adminOnly && !isAdmin) {
                return null
              }
              const isActive = matchRoute({ to: item.to })
              return (
                <SidebarMenuItem key={item.to}>
                  <SidebarMenuButton isActive={!!isActive} render={<Link to={item.to} />} tooltip={t(item.labelKey)}>
                    <item.icon />
                    <span>{t(item.labelKey)}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )
            })}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger render={<SidebarMenuButton className="aria-expanded:bg-muted" size="lg" />}>
                <Avatar>
                  <AvatarFallback>{user?.username?.charAt(0).toUpperCase() ?? 'U'}</AvatarFallback>
                </Avatar>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-medium">{user?.username ?? 'User'}</span>
                  <span className="truncate text-xs capitalize">{user?.role ?? ''}</span>
                </div>
                <ChevronsUpDown className="ml-auto size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="end"
                className="min-w-56 rounded-lg"
                side={isMobile ? 'bottom' : 'right'}
                sideOffset={4}
              >
                <DropdownMenuLabel className="p-0 font-normal">
                  <div className="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                    <Avatar>
                      <AvatarFallback>{user?.username?.charAt(0).toUpperCase() ?? 'U'}</AvatarFallback>
                    </Avatar>
                    <div className="grid flex-1 text-left text-sm leading-tight">
                      <span className="truncate font-medium">{user?.username ?? 'User'}</span>
                      <span className="truncate text-xs capitalize">{user?.role ?? ''}</span>
                    </div>
                  </div>
                </DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={handleLogout}>
                  <LogOut />
                  {t('logout')}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  )
}
