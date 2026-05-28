import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useApiMutation, useApiQuery } from '../src/hooks/escape-hatch'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

describe('api hooks', () => {
  let qc: QueryClient
  beforeEach(() => {
    resetRuntime()
    qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: qc,
      serversStore: () => [],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {}
    })
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ data: { hello: 'world' } })
    }) as any
  })

  const wrapper = ({ children }: any) => createElement(QueryClientProvider, { client: qc, children })

  it('useApiQuery unwraps {data}', async () => {
    const { result } = renderHook(() => useApiQuery<{ hello: string }>('/api/test'), { wrapper })
    await waitFor(() => expect(result.current.data).toEqual({ hello: 'world' }))
  })

  it('useApiMutation calls fetch with method+body', async () => {
    const { result } = renderHook(() => useApiMutation<{ ok: true }, { x: number }>('POST', '/api/do'), { wrapper })
    await result.current.mutateAsync({ x: 1 })
    expect(global.fetch).toHaveBeenCalledWith(
      '/api/do',
      expect.objectContaining({ method: 'POST', credentials: 'include' })
    )
  })
})
