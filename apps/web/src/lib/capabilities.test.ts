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
  classifyCapability,
  getEffectiveCapabilityEnabled,
  hasCap,
  temporaryGrantFor
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

const base = { capabilities: CAP_DEFAULT, effective_capabilities: CAP_DEFAULT }

describe('classifyCapability', () => {
  it('returns off when the bit is not set', () => {
    expect(classifyCapability(base, CAP_TERMINAL)).toBe('off')
  })

  it('returns temporary when a matching active grant exists', () => {
    const server = {
      // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
      capabilities: CAP_DEFAULT | CAP_TERMINAL,
      // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
      effective_capabilities: CAP_DEFAULT | CAP_TERMINAL,
      temporary: [{ cap: 'terminal', granted_at: 0, expires_at: 9_999_999_999 }]
    }
    expect(classifyCapability(server, CAP_TERMINAL)).toBe('temporary')
    expect(temporaryGrantFor(server, CAP_TERMINAL)?.expires_at).toBe(9_999_999_999)
  })

  it('returns enabled when the bit is set but not via a grant', () => {
    const server = {
      // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
      capabilities: CAP_DEFAULT | CAP_TERMINAL,
      // biome-ignore lint/suspicious/noBitwiseOperators: capability bitmask
      effective_capabilities: CAP_DEFAULT | CAP_TERMINAL
    }
    expect(classifyCapability(server, CAP_TERMINAL)).toBe('enabled')
  })
})
