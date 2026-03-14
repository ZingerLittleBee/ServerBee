import { describe, expect, it } from 'vitest'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from './utils'

describe('countryCodeToFlag', () => {
  it('converts US to flag emoji', () => {
    expect(countryCodeToFlag('US')).toBe('🇺🇸')
  })
  it('converts CN to flag emoji', () => {
    expect(countryCodeToFlag('CN')).toBe('🇨🇳')
  })
  it('returns empty for null', () => {
    expect(countryCodeToFlag(null)).toBe('')
  })
  it('returns empty for undefined', () => {
    expect(countryCodeToFlag(undefined)).toBe('')
  })
  it('returns empty for single char', () => {
    expect(countryCodeToFlag('A')).toBe('')
  })
  it('handles lowercase', () => {
    expect(countryCodeToFlag('gb')).toBe('🇬🇧')
  })
})

describe('formatBytes', () => {
  it('returns "0 B" for 0', () => {
    expect(formatBytes(0)).toBe('0 B')
  })
  it('returns "0 B" for negative', () => {
    expect(formatBytes(-100)).toBe('0 B')
  })
  it('returns "0 B" for NaN', () => {
    expect(formatBytes(Number.NaN)).toBe('0 B')
  })
  it('formats KB', () => {
    expect(formatBytes(1024)).toBe('1.0 KB')
  })
  it('formats MB', () => {
    expect(formatBytes(1_048_576)).toBe('1.0 MB')
  })
  it('formats GB', () => {
    expect(formatBytes(1_073_741_824)).toBe('1.0 GB')
  })
  it('formats TB', () => {
    expect(formatBytes(1_099_511_627_776)).toBe('1.0 TB')
  })
  it('formats fractional values', () => {
    expect(formatBytes(1536)).toBe('1.5 KB')
  })
})

describe('formatSpeed', () => {
  it('appends /s suffix', () => {
    expect(formatSpeed(1024)).toBe('1.0 KB/s')
  })
  it('handles zero', () => {
    expect(formatSpeed(0)).toBe('0 B/s')
  })
})

describe('formatUptime', () => {
  it('formats days and hours', () => {
    expect(formatUptime(90_000)).toBe('1d 1h')
  })
  it('formats exactly one day', () => {
    expect(formatUptime(86_400)).toBe('1d 0h')
  })
  it('formats hours and minutes', () => {
    expect(formatUptime(3900)).toBe('1h 5m')
  })
  it('formats minutes only', () => {
    expect(formatUptime(300)).toBe('5m')
  })
  it('formats zero seconds', () => {
    expect(formatUptime(0)).toBe('0m')
  })
})
