import type { QueryClient } from '@tanstack/react-query'

export interface ServerSummary {
  capabilities: number
  id: string
  lastSeen: number | null
  name: string
  online: boolean
}

export interface NotifyOptions {
  message: string
  type: 'success' | 'error' | 'info'
}

export interface ConfirmOptions {
  body?: string
  title: string
}

export interface ThemeSnapshot {
  cssVar: (name: string) => string
  mode: 'light' | 'dark'
}

export interface WidgetRuntime {
  apiBaseUrl: string
  /** Optional: hint list of well-known metric paths for the metricPath picker. */
  getMetricPaths?: () => string[]
  /** Optional: host-provided toast hook. SDK falls back to console.*/
  notify?: (opts: NotifyOptions) => void
  onConfigUpdate: (instanceId: string, patch: Record<string, unknown>) => void
  queryClient: QueryClient
  /** Optional: host-provided confirm dialog. SDK falls back to window.confirm. */
  requestConfirm?: (opts: ConfirmOptions) => Promise<boolean>
  serverByIdStore: (id: string) => unknown
  serversStore: () => ServerSummary[]
  /** Subscribe to server-list changes. Returns an unsubscribe fn. */
  subscribeServers: (cb: () => void) => () => void
  /** Subscribe to theme changes. Returns an unsubscribe fn. */
  subscribeTheme: (cb: () => void) => () => void
  themeStore: () => ThemeSnapshot
}

let _runtime: WidgetRuntime | null = null

type RuntimeInput = Omit<WidgetRuntime, 'serverByIdStore' | 'subscribeServers' | 'subscribeTheme'> & {
  serverByIdStore?: WidgetRuntime['serverByIdStore']
  subscribeServers?: WidgetRuntime['subscribeServers']
  subscribeTheme?: WidgetRuntime['subscribeTheme']
}

const noopSubscribe = (_cb: () => void): (() => void) => {
  return () => {
    /* no-op */
  }
}

export function createWidgetRuntime(rt: RuntimeInput): WidgetRuntime {
  _runtime = {
    serverByIdStore: () => undefined,
    subscribeServers: noopSubscribe,
    subscribeTheme: noopSubscribe,
    ...rt
  }
  return _runtime
}

export function getRuntime(): WidgetRuntime {
  if (!_runtime) {
    throw new Error('widget-sdk: runtime not installed (host bridge missing)')
  }
  return _runtime
}

export function resetRuntime(): void {
  _runtime = null
}
