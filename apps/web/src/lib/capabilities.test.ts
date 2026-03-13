import { describe, expect, it } from 'vitest'
import { CAP_DEFAULT, CAP_EXEC, CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP, CAP_TERMINAL, hasCap } from './capabilities'

describe('capability toggles', () => {
  it('default capabilities have ping enabled, terminal disabled', () => {
    expect(hasCap(CAP_DEFAULT, CAP_TERMINAL)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_EXEC)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_PING_ICMP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_TCP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_HTTP)).toBe(true)
  })

  it('toggle on adds bit', () => {
    const caps = CAP_DEFAULT
    // biome-ignore lint/style/noNonNullAssertion: test code
    const newCaps = caps | CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(true)
  })

  it('toggle off removes bit', () => {
    // biome-ignore lint/style/noNonNullAssertion: test code
    const caps = CAP_DEFAULT | CAP_TERMINAL
    // biome-ignore lint/style/noNonNullAssertion: test code
    const newCaps = caps & ~CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(false)
  })
})
