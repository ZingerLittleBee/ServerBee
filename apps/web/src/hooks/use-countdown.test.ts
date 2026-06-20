import { describe, expect, it } from 'vitest'
import { formatCountdown } from './use-countdown'

describe('formatCountdown', () => {
  it('formats zero as 0:00', () => {
    expect(formatCountdown(0)).toBe('0:00')
  })

  it('zero-pads seconds under a minute', () => {
    expect(formatCountdown(5)).toBe('0:05')
  })

  it('formats minutes and seconds', () => {
    expect(formatCountdown(65)).toBe('1:05')
  })

  it('formats just under an hour as m:ss', () => {
    expect(formatCountdown(3599)).toBe('59:59')
  })

  it('switches to Hh Mm at one hour', () => {
    expect(formatCountdown(3661)).toBe('1h 1m')
  })

  it('shows zero minutes at a whole hour boundary', () => {
    expect(formatCountdown(7200)).toBe('2h 0m')
  })
})
