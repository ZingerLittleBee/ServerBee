import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAuth } from './use-auth'

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

const mockUser = {
  user_id: 'u-1',
  username: 'admin',
  role: 'admin'
}

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

function mockFetchResponse(body: unknown, options: { status?: number } = {}) {
  const { status = 200 } = options
  return vi.mocked(globalThis.fetch).mockResolvedValueOnce(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' }
    })
  )
}

describe('useAuth', () => {
  it('returns unauthenticated state when /api/auth/me fails', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      new Response('Unauthorized', { status: 401, statusText: 'Unauthorized' })
    )

    const { result } = renderHook(() => useAuth(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false)
    })

    expect(result.current.isAuthenticated).toBe(false)
    expect(result.current.user).toBeNull()
  })

  it('returns authenticated state when /api/auth/me succeeds', async () => {
    mockFetchResponse({ data: mockUser })

    const { result } = renderHook(() => useAuth(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false)
    })

    expect(result.current.isAuthenticated).toBe(true)
    expect(result.current.user).toEqual(mockUser)
  })

  it('updates user after successful login', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      new Response('Unauthorized', { status: 401, statusText: 'Unauthorized' })
    )

    const { result } = renderHook(() => useAuth(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false)
    })

    expect(result.current.user).toBeNull()

    mockFetchResponse({ data: mockUser })

    await result.current.login({ username: 'admin', password: 'secret' })

    await waitFor(() => {
      expect(result.current.user).toEqual(mockUser)
    })

    expect(result.current.isAuthenticated).toBe(true)
  })

  it('clears user after logout', async () => {
    mockFetchResponse({ data: mockUser })

    const { result } = renderHook(() => useAuth(), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isAuthenticated).toBe(true)
    })

    vi.mocked(globalThis.fetch).mockResolvedValueOnce(new Response(null, { status: 204 }))

    await result.current.logout()

    await waitFor(() => {
      expect(result.current.user).toBeNull()
    })

    expect(result.current.isAuthenticated).toBe(false)
  })
})
