// Widget modules dynamically `import *` from these packages via the import-map
// shim, so the host must expose the full namespace objects on `globalThis`.
// Named imports won't work here — these `*` imports are intentional.
import type { ConfirmOptions, NotifyOptions, ServerSummary, ThemeSnapshot } from '@serverbee/widget-sdk'
// biome-ignore lint/performance/noNamespaceImport: shim requires full namespace
import * as Sdk from '@serverbee/widget-sdk'
import type { QueryClient } from '@tanstack/react-query'
// biome-ignore lint/performance/noNamespaceImport: shim requires full namespace
import * as React from 'react'
// biome-ignore lint/performance/noNamespaceImport: shim requires full namespace
import * as JsxRuntime from 'react/jsx-runtime'
// biome-ignore lint/performance/noNamespaceImport: shim requires full namespace
import * as ReactDOM from 'react-dom'
import { toast } from 'sonner'
import type { ServerMetrics } from '@/hooks/use-servers-ws'

const SERVERS_QUERY_KEY = ['servers'] as const

export interface BridgeInputs {
  onConfigUpdate?: (instanceId: string, patch: Record<string, unknown>) => void
  queryClient: QueryClient
  /** Optional: host confirm hook. If omitted, the SDK falls back to window.confirm. */
  requestConfirm?: (opts: ConfirmOptions) => Promise<boolean>
}

function metricsToSummary(metrics: ServerMetrics): ServerSummary {
  return {
    id: metrics.id,
    name: metrics.name,
    online: metrics.online,
    lastSeen: typeof metrics.last_active === 'number' ? metrics.last_active : null,
    capabilities:
      typeof metrics.effective_capabilities === 'number' ? metrics.effective_capabilities : (metrics.capabilities ?? 0)
  }
}

interface ServerSummariesCache {
  raw: ServerMetrics[] | undefined
  summaries: ServerSummary[]
}

/**
 * Memoized projection of the `['servers']` cache → `ServerSummary[]`. We cache
 * by the raw array reference so `useSyncExternalStore` sees a stable snapshot
 * (otherwise React loops forever).
 */
function makeServerSummariesGetter(queryClient: QueryClient): () => ServerSummary[] {
  let cache: ServerSummariesCache = { raw: undefined, summaries: [] }
  return () => {
    const raw = queryClient.getQueryData<ServerMetrics[]>(SERVERS_QUERY_KEY)
    if (raw === cache.raw) {
      return cache.summaries
    }
    const summaries = raw ? raw.map(metricsToSummary) : []
    cache = { raw, summaries }
    return summaries
  }
}

function makeServerByIdGetter(queryClient: QueryClient): (id: string) => unknown {
  return (id: string) => {
    const raw = queryClient.getQueryData<ServerMetrics[]>(SERVERS_QUERY_KEY)
    return raw?.find((s) => s.id === id)
  }
}

function makeSubscribeServers(queryClient: QueryClient): (cb: () => void) => () => void {
  return (cb: () => void) => {
    const unsub = queryClient.getQueryCache().subscribe((event) => {
      // We only care about updates to the ['servers'] cache. The cache emits
      // a flurry of events (added/removed/updated/observers); filter to those
      // that mutate the data for our queryKey.
      const key = event.query.queryKey
      if (Array.isArray(key) && key.length >= 1 && key[0] === SERVERS_QUERY_KEY[0]) {
        cb()
      }
    })
    return unsub
  }
}

interface ThemeWatcher {
  getSnapshot: () => ThemeSnapshot
  subscribe: (cb: () => void) => () => void
}

/**
 * The host app uses a manual `light`/`dark` class toggle on `<html>` driven by
 * `ThemeProvider`. We mirror that into a `ThemeSnapshot` and re-emit on
 * `MutationObserver` `class` changes — same source of truth, no extra coupling.
 */
function makeThemeWatcher(): ThemeWatcher {
  const listeners = new Set<() => void>()
  let observer: MutationObserver | null = null
  let storageHandler: ((e: StorageEvent) => void) | null = null
  let mediaQuery: MediaQueryList | null = null
  let mediaHandler: (() => void) | null = null

  function readMode(): 'light' | 'dark' {
    return typeof document !== 'undefined' && document.documentElement.classList.contains('dark') ? 'dark' : 'light'
  }

  // We memoize the snapshot **object** by current mode so getSnapshot returns
  // a stable reference across calls when the underlying mode is unchanged.
  // `cssVar` is a pure read of the live document; capturing it inside the
  // snapshot object is fine.
  let lastMode: 'light' | 'dark' = readMode()
  let cached: ThemeSnapshot = makeSnapshot(lastMode)

  function makeSnapshot(mode: 'light' | 'dark'): ThemeSnapshot {
    return {
      mode,
      cssVar: (name: string) =>
        typeof getComputedStyle !== 'undefined'
          ? getComputedStyle(document.documentElement).getPropertyValue(name).trim()
          : ''
    }
  }

  function getSnapshot(): ThemeSnapshot {
    const mode = readMode()
    if (mode !== lastMode) {
      lastMode = mode
      cached = makeSnapshot(mode)
    }
    return cached
  }

  function notify() {
    const mode = readMode()
    if (mode !== lastMode) {
      lastMode = mode
      cached = makeSnapshot(mode)
      for (const cb of listeners) {
        cb()
      }
    }
  }

  function ensureObservers() {
    if (typeof document === 'undefined') {
      return
    }
    if (!observer) {
      observer = new MutationObserver(() => notify())
      observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
    }
    if (!storageHandler) {
      storageHandler = (e: StorageEvent) => {
        if (e.key === 'theme') {
          notify()
        }
      }
      window.addEventListener('storage', storageHandler)
    }
    if (!mediaQuery) {
      mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
      mediaHandler = () => notify()
      mediaQuery.addEventListener('change', mediaHandler)
    }
  }

  function teardownIfUnused() {
    if (listeners.size > 0) {
      return
    }
    if (observer) {
      observer.disconnect()
      observer = null
    }
    if (storageHandler) {
      window.removeEventListener('storage', storageHandler)
      storageHandler = null
    }
    if (mediaQuery && mediaHandler) {
      mediaQuery.removeEventListener('change', mediaHandler)
      mediaQuery = null
      mediaHandler = null
    }
  }

  return {
    getSnapshot,
    subscribe: (cb: () => void) => {
      listeners.add(cb)
      ensureObservers()
      return () => {
        listeners.delete(cb)
        teardownIfUnused()
      }
    }
  }
}

function notifyViaToast(opts: NotifyOptions): void {
  switch (opts.type) {
    case 'success':
      toast.success(opts.message)
      break
    case 'error':
      toast.error(opts.message)
      break
    default:
      toast(opts.message)
      break
  }
}

export function mountRuntimeBridge(inputs: BridgeInputs): void {
  ;(globalThis as Record<string, unknown>).__SERVERBEE_REACT__ = React
  ;(globalThis as Record<string, unknown>).__SERVERBEE_REACT_DOM__ = ReactDOM
  ;(globalThis as Record<string, unknown>).__SERVERBEE_JSX_RUNTIME__ = JsxRuntime
  ;(globalThis as Record<string, unknown>).__SERVERBEE_SDK__ = Sdk

  const serversStore = makeServerSummariesGetter(inputs.queryClient)
  const serverByIdStore = makeServerByIdGetter(inputs.queryClient)
  const subscribeServers = makeSubscribeServers(inputs.queryClient)
  const themeWatcher = makeThemeWatcher()

  Sdk.createWidgetRuntime({
    apiBaseUrl: '/api',
    queryClient: inputs.queryClient,
    serversStore,
    serverByIdStore,
    subscribeServers,
    themeStore: themeWatcher.getSnapshot,
    subscribeTheme: themeWatcher.subscribe,
    notify: notifyViaToast,
    requestConfirm: inputs.requestConfirm,
    onConfigUpdate: inputs.onConfigUpdate ?? (() => undefined)
  })
}
