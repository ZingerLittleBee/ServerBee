import type { ServerSummary } from '@serverbee/widget-sdk'
import * as Sdk from '@serverbee/widget-sdk'
import type { QueryClient } from '@tanstack/react-query'
import * as React from 'react'
import * as JsxRuntime from 'react/jsx-runtime'
import * as ReactDOM from 'react-dom'

export interface BridgeInputs {
  onConfigUpdate: (instanceId: string, patch: Record<string, unknown>) => void
  queryClient: QueryClient
  serverByIdStore: (id: string) => unknown
  serversStore: () => ServerSummary[]
  themeStore: () => { mode: 'light' | 'dark'; cssVar: (n: string) => string }
}

export function mountRuntimeBridge(inputs: BridgeInputs): void {
  ;(globalThis as any).__SERVERBEE_REACT__ = React
  ;(globalThis as any).__SERVERBEE_REACT_DOM__ = ReactDOM
  ;(globalThis as any).__SERVERBEE_JSX_RUNTIME__ = JsxRuntime
  ;(globalThis as any).__SERVERBEE_SDK__ = Sdk

  Sdk.createWidgetRuntime({
    apiBaseUrl: '/api',
    queryClient: inputs.queryClient,
    serversStore: inputs.serversStore,
    serverByIdStore: inputs.serverByIdStore,
    themeStore: inputs.themeStore,
    onConfigUpdate: inputs.onConfigUpdate
  })
}
