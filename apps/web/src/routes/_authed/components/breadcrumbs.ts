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

export interface BreadcrumbEntry {
  label: string
  to?: string
}

const SERVER_DETAIL_PATH_RE = /^\/servers\/([^/]+)\/?$/
const TRAILING_SLASH_RE = /\/$/

export function getServerDetailId(pathname: string): string {
  return SERVER_DETAIL_PATH_RE.exec(pathname)?.[1] ?? ''
}

export function buildBreadcrumbs(
  pathname: string,
  translate: (key: string) => string,
  serverDetailName?: string
): BreadcrumbEntry[] {
  // Dashboard owns the title bar (leading switcher + trailing edit controls);
  // keep the crumb list empty so the header does not also render "Dashboard".
  if (pathname === '/') {
    return []
  }

  const serverDetailId = getServerDetailId(pathname)
  const segments = pathname.replace(TRAILING_SLASH_RE, '').split('/').filter(Boolean)
  const crumbs: BreadcrumbEntry[] = []

  let accumulated = ''
  for (let i = 0; i < segments.length; i++) {
    accumulated += `/${segments[i]}`
    const labelKey = ROUTE_LABELS[accumulated]
    const dynamicLabel = serverDetailId && i === segments.length - 1 ? serverDetailName || serverDetailId : undefined
    const isLast = i === segments.length - 1

    if (labelKey || dynamicLabel) {
      crumbs.push({
        label: dynamicLabel ?? translate(labelKey),
        to: isLast ? undefined : accumulated
      })
    }
  }

  if (crumbs.length === 0) {
    const firstSegment = segments[0]
    const parentKey = ROUTE_LABELS[`/${firstSegment}`]
    if (parentKey) {
      crumbs.push({ label: translate(parentKey), to: `/${firstSegment}` })
    }
  }

  return crumbs
}
