import { describe, expect, it } from 'vitest'
import { buildEmailPayload } from './notifications'

describe('buildEmailPayload', () => {
  it('wraps a single recipient as a string array', () => {
    const payload = buildEmailPayload('alerts@example.com', ['ops@example.com'])
    expect(payload).toEqual({ from: 'alerts@example.com', to: ['ops@example.com'] })
  })

  it('preserves multiple recipients in order', () => {
    const payload = buildEmailPayload('alerts@example.com', ['a@x.com', 'b@y.com', 'c@z.com'])
    expect(payload.to).toEqual(['a@x.com', 'b@y.com', 'c@z.com'])
  })

  it('allows an empty from (validation happens at submit time)', () => {
    const payload = buildEmailPayload('', ['ops@example.com'])
    expect(payload.from).toBe('')
  })
})
