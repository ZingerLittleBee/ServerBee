import { getRuntime } from '../runtime-context'

export function useTheme() {
  return getRuntime().themeStore()
}

export function useConfigUpdate<TConfig = Record<string, unknown>>(instanceId: string) {
  const runtime = getRuntime()
  return (patch: Partial<TConfig>) => runtime.onConfigUpdate(instanceId, patch as Record<string, unknown>)
}
