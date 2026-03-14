import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, api } from './api-client'

const mockFetch = vi.fn()
globalThis.fetch = mockFetch

beforeEach(() => {
  mockFetch.mockReset()
})

describe('api.get', () => {
  it('unwraps { data: T } response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: { id: '1', name: 'srv' } })
    })
    const result = await api.get<{ id: string; name: string }>('/api/servers/1')
    expect(result).toEqual({ id: '1', name: 'srv' })
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/servers/1',
      expect.objectContaining({ method: 'GET', credentials: 'include' })
    )
  })

  it('returns raw object when no data wrapper', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ key: 'value' })
    })
    const result = await api.get<{ key: string }>('/api/test')
    expect(result).toEqual({ key: 'value' })
  })
})

describe('api.post', () => {
  it('serializes body as JSON', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: { token: 'abc' } })
    })
    await api.post('/api/auth/login', { username: 'admin', password: 'pass' })
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/auth/login',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ username: 'admin', password: 'pass' }),
        headers: { 'Content-Type': 'application/json' }
      })
    )
  })
})

describe('api.delete', () => {
  it('returns undefined for 204 No Content', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 204
    })
    const result = await api.delete('/api/servers/1')
    expect(result).toBeUndefined()
  })
})

describe('error handling', () => {
  it('throws ApiError with status on 401', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 401,
      statusText: 'Unauthorized',
      text: async () => 'Invalid credentials'
    })
    await expect(api.get('/api/auth/status')).rejects.toThrow(ApiError)
  })

  it('ApiError contains status code and message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      text: async () => 'Server error'
    })
    try {
      await api.get('/api/broken')
      expect.unreachable('should have thrown')
    } catch (e) {
      expect(e).toBeInstanceOf(ApiError)
      expect((e as ApiError).status).toBe(500)
      expect((e as ApiError).message).toBe('Server error')
    }
  })
})
