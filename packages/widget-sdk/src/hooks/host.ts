import { useSyncExternalStore } from 'react'
import { getRuntime, type ThemeSnapshot } from '../runtime-context'

export function useTheme(): ThemeSnapshot {
  const rt = getRuntime()
  return useSyncExternalStore(rt.subscribeTheme, rt.themeStore, rt.themeStore)
}

export function useConfigUpdate<TConfig = Record<string, unknown>>(instanceId: string) {
  const runtime = getRuntime()
  return (patch: Partial<TConfig>) => runtime.onConfigUpdate(instanceId, patch as Record<string, unknown>)
}
