import { isCompatible, SDK_VERSION, type WidgetManifest, type WidgetModule } from '@serverbee/widget-sdk'
import { registryActions } from './registry'

interface ListEntry {
  entry_path: string
  id: string
  manifest: WidgetManifest
  version: string
}

export interface BootstrapOptions {
  baseUrl?: string
  /** Override the host SDK version (used in tests). */
  hostSdkVersion?: string
  /** Override the import function (used in tests). */
  importer?: (url: string) => Promise<{ default: WidgetModule }>
}

export async function bootstrapLoader(opts: BootstrapOptions = {}): Promise<void> {
  const base = opts.baseUrl ?? '/api/widget-modules'
  const importer = opts.importer ?? ((url: string) => import(/* @vite-ignore */ url))
  const hostVersion = opts.hostSdkVersion ?? SDK_VERSION

  const res = await fetch(base, { credentials: 'include' })
  if (!res.ok) {
    throw new Error(`bootstrapLoader: list failed ${res.status}`)
  }
  const body = (await res.json()) as { data: ListEntry[] }
  const modules = body.data

  await Promise.allSettled(
    modules.map(async (entry) => {
      try {
        if (!isCompatible(hostVersion, entry.manifest.sdkVersion)) {
          throw new Error(
            `sdk version mismatch: host ${hostVersion} does not satisfy manifest range ${entry.manifest.sdkVersion}`
          )
        }
        const url = `${base}/${entry.id}/${entry.entry_path}`
        const mod = await importer(url)
        if (!mod.default || mod.default.__brand !== 'WidgetModule') {
          throw new Error(`module ${entry.id} did not export a WidgetModule via default`)
        }
        registryActions.register(entry.id, mod.default, entry.manifest)
      } catch (err) {
        registryActions.recordLoadFailure(entry.id, err instanceof Error ? err : new Error(String(err)))
      }
    })
  )
}
