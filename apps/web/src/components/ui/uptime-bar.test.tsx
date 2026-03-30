import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { UptimeBar } from './uptime-bar'

function greenColor(v: number | null): string {
  if (v == null || v >= 100) {
    return '#ef4444'
  }
  if (v >= 50) {
    return '#f59e0b'
  }
  return '#10b981'
}

describe('UptimeBar', () => {
  it('renders one bar per data point', () => {
    const { container } = render(<UptimeBar data={[10, 20, 30]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(3)
  })

  it('renders nothing when data is empty', () => {
    const { container } = render(<UptimeBar data={[]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(0)
  })

  it('applies color from getColor callback', () => {
    const { container } = render(<UptimeBar data={[10, 80, null]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.backgroundColor).toBe('rgb(16, 185, 129)')
    expect((bars[1] as HTMLElement).style.backgroundColor).toBe('rgb(245, 158, 11)')
    expect((bars[2] as HTMLElement).style.backgroundColor).toBe('rgb(239, 68, 68)')
  })

  it('renders null values at 100% height', () => {
    const { container } = render(<UptimeBar data={[50, null]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('scales bar heights relative to maxValue', () => {
    const { container } = render(<UptimeBar data={[50, 100]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.height).toBe('50%')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('uses data max when maxValue not provided', () => {
    const { container } = render(<UptimeBar data={[25, 50]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    // 25/50 = 50%, 50/50 = 100%
    expect((bars[0] as HTMLElement).style.height).toBe('50%')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('enforces minimum 10% height for non-null non-zero values', () => {
    const { container } = render(<UptimeBar data={[1, 100]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.height).toBe('10%')
  })

  it('enforces minimum 10% height for zero values', () => {
    const { container } = render(<UptimeBar data={[0, 100]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.height).toBe('10%')
  })

  it('has accessible label', () => {
    render(<UptimeBar ariaLabel="Latency trend" data={[10, 20]} getColor={greenColor} />)
    expect(screen.getByLabelText('Latency trend')).toBeDefined()
  })
})
