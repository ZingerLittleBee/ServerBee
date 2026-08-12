import { afterEach, describe, expect, it, vi } from 'vitest'
import { randomUUID } from './uuid'

describe('randomUUID', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('uses the native implementation when the origin exposes it', () => {
    const nativeValue = '123e4567-e89b-42d3-a456-426614174000'
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => nativeValue),
      getRandomValues: vi.fn()
    })

    expect(randomUUID()).toBe(nativeValue)
    expect(crypto.randomUUID).toHaveBeenCalledOnce()
  })

  it('creates an RFC 4122 v4 UUID when randomUUID is unavailable', () => {
    vi.stubGlobal('crypto', {
      getRandomValues: (bytes: Uint8Array) => {
        bytes.set([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
        return bytes
      }
    })

    expect(randomUUID()).toBe('00010203-0405-4607-8809-0a0b0c0d0e0f')
  })
})
