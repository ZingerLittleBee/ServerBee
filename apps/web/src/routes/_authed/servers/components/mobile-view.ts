export type ServersViewMode = 'grid' | 'table'

interface ResolveInitialServersViewInput {
  isMobile: boolean
  searchView?: ServersViewMode
  storedView: string | null
}

function isServersViewMode(value: string | null | undefined): value is ServersViewMode {
  return value === 'grid' || value === 'table'
}

export function resolveInitialServersView({
  isMobile,
  searchView,
  storedView
}: ResolveInitialServersViewInput): ServersViewMode {
  if (isServersViewMode(searchView)) {
    return searchView
  }
  if (isServersViewMode(storedView)) {
    return storedView
  }
  return isMobile ? 'grid' : 'table'
}

export function getInitialServersView(searchView?: ServersViewMode): ServersViewMode {
  const storedView = typeof window === 'undefined' ? null : localStorage.getItem('serverbee-servers-view-mode')
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768
  return resolveInitialServersView({ isMobile, searchView, storedView })
}
