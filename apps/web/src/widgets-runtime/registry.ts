import type { WidgetManifest, WidgetModule } from '@serverbee/widget-sdk'
import { create } from 'zustand'

export interface RegistryEntry {
  manifest: WidgetManifest
  module: WidgetModule
}

interface RegistryState {
  failures: Map<string, Error>
  modules: Map<string, RegistryEntry>
}

export const useWidgetRegistry = create<RegistryState>(() => ({
  modules: new Map(),
  failures: new Map()
}))

export const registryActions = {
  register(id: string, module: WidgetModule, manifest: WidgetManifest) {
    useWidgetRegistry.setState((state) => {
      const modules = new Map(state.modules)
      modules.set(id, { manifest, module })
      const failures = new Map(state.failures)
      failures.delete(id)
      return { modules, failures }
    })
  },
  unregister(id: string) {
    useWidgetRegistry.setState((state) => {
      const modules = new Map(state.modules)
      modules.delete(id)
      return { modules }
    })
  },
  get(id: string): RegistryEntry | undefined {
    return useWidgetRegistry.getState().modules.get(id)
  },
  list(): RegistryEntry[] {
    return Array.from(useWidgetRegistry.getState().modules.values())
  },
  recordLoadFailure(id: string, err: Error) {
    useWidgetRegistry.setState((state) => {
      const failures = new Map(state.failures)
      failures.set(id, err)
      return { failures }
    })
  }
}
