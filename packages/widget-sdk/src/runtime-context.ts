import type { QueryClient } from '@tanstack/react-query'

export interface ServerSummary {
  capabilities: number
  id: string
  lastSeen: number | null
  name: string
  online: boolean
}

export interface WidgetRuntime {
  apiBaseUrl: string
  onConfigUpdate: (instanceId: string, patch: Record<string, unknown>) => void
  queryClient: QueryClient
  serverByIdStore: (id: string) => unknown
  serversStore: () => ServerSummary[]
  themeStore: () => { mode: 'light' | 'dark'; cssVar: (name: string) => string }
}

let _runtime: WidgetRuntime | null = null

export function createWidgetRuntime(
  rt: Omit<WidgetRuntime, 'serverByIdStore'> & {
    serverByIdStore?: WidgetRuntime['serverByIdStore']
  }
): WidgetRuntime {
  _runtime = {
    serverByIdStore: () => undefined,
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
