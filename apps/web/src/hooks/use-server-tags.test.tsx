import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useServerTags, useUpdateServerTags } from './use-server-tags'

function harness() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const Wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  )
  return { qc, Wrapper }
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useServerTags', () => {
  it('fetches GET /api/servers/:id/tags', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
      new Response(JSON.stringify({ data: ['a', 'b'] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' }
      })
    )
    const { Wrapper } = harness()
    const { result } = renderHook(() => useServerTags('srv-1'), { wrapper: Wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toEqual(['a', 'b'])
  })
})

describe('useUpdateServerTags', () => {
  it('PUTs tags and patches both caches on success', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementationOnce((_input, init) => {
      const body = JSON.parse((init as RequestInit).body as string) as { tags: string[] }
      return Promise.resolve(
        new Response(JSON.stringify({ data: [...body.tags].sort() }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' }
        })
      )
    })
    const { qc, Wrapper } = harness()
    qc.setQueryData(['server-tags', 'srv-1'], ['old'])
    qc.setQueryData(['servers'], [{ id: 'srv-1', tags: ['old'] }])
    const { result } = renderHook(() => useUpdateServerTags('srv-1'), { wrapper: Wrapper })
    await result.current.mutateAsync(['b', 'a'])
    expect(qc.getQueryData(['server-tags', 'srv-1'])).toEqual(['a', 'b'])
    expect((qc.getQueryData(['servers']) as Array<{ id: string; tags: string[] }>)[0].tags).toEqual(['a', 'b'])
  })
})
