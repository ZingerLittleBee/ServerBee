import { describe, expect, it } from 'vitest'
import {
  CAP_DEFAULT,
  CAP_EXEC,
  CAP_FILE,
  CAP_PING_HTTP,
  CAP_PING_ICMP,
  CAP_PING_TCP,
  CAP_TERMINAL,
  CAP_UPGRADE,
  CAPABILITIES,
  getEffectiveCapabilityEnabled,
  hasCap,
  isClientCapabilityLocked
} from './capabilities'

describe('capability toggles', () => {
  it('default capabilities enable upgrade alongside the low-risk probes', () => {
    expect(hasCap(CAP_DEFAULT, CAP_TERMINAL)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_EXEC)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_UPGRADE)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_ICMP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_TCP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_HTTP)).toBe(true)
  })

  it('classifies auto upgrade as a low-risk capability', () => {
    const upgradeCapability = CAPABILITIES.find(({ key }) => key === 'upgrade')

    expect(upgradeCapability).toMatchObject({
      bit: CAP_UPGRADE,
      risk: 'low'
    })
  })

  it('toggle on adds bit', () => {
    const caps = CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    const newCaps = caps | CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(true)
  })

  it('toggle off removes bit', () => {
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    const caps = CAP_DEFAULT | CAP_TERMINAL
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    const newCaps = caps & ~CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(false)
  })

  it('detects client capability locks', () => {
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    expect(isClientCapabilityLocked(CAP_DEFAULT | CAP_FILE, CAP_FILE)).toBe(false)
    expect(isClientCapabilityLocked(CAP_DEFAULT, CAP_FILE)).toBe(true)
    expect(isClientCapabilityLocked(undefined, CAP_FILE)).toBe(false)
  })

  it('prefers effective capabilities when present', () => {
    expect(getEffectiveCapabilityEnabled(CAP_FILE, CAP_DEFAULT, CAP_FILE)).toBe(true)
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    expect(getEffectiveCapabilityEnabled(0, CAP_DEFAULT | CAP_FILE, CAP_FILE)).toBe(false)
  })

  it('falls back to configured capabilities when runtime effective capabilities are absent', () => {
    // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
    expect(getEffectiveCapabilityEnabled(undefined, CAP_DEFAULT | CAP_EXEC, CAP_EXEC)).toBe(true)
    expect(getEffectiveCapabilityEnabled(null, CAP_DEFAULT, CAP_EXEC)).toBe(false)
  })
})
