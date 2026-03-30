import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { RingChart } from './ring-chart'

describe('RingChart', () => {
  it('renders percentage text', () => {
    render(<RingChart color="#3b82f6" label="CPU" value={72.3} />)
    expect(screen.getByText('72.3%')).toBeDefined()
  })

  it('renders label', () => {
    render(<RingChart color="#3b82f6" label="CPU" value={50} />)
    expect(screen.getByText('CPU')).toBeDefined()
  })

  it('renders SVG with accessible role and label', () => {
    render(<RingChart color="#3b82f6" label="MEM" value={85} />)
    const svg = screen.getByRole('img')
    expect(svg.getAttribute('aria-label')).toBe('MEM 85.0%')
  })

  it('clamps value to 0-100 range', () => {
    const { rerender } = render(<RingChart color="#3b82f6" label="CPU" value={150} />)
    expect(screen.getByText('100.0%')).toBeDefined()

    rerender(<RingChart color="#3b82f6" label="CPU" value={-10} />)
    expect(screen.getByText('0.0%')).toBeDefined()
  })

  it('accepts custom size', () => {
    const { container } = render(<RingChart color="#3b82f6" label="CPU" size={40} value={50} />)
    const wrapper = container.firstElementChild as HTMLElement
    expect(wrapper.style.width).toBe('40px')
  })

  it('renders SVG with explicit width and height attributes', () => {
    const { container } = render(<RingChart color="#3b82f6" label="CPU" size={48} value={50} />)
    const svg = container.querySelector('svg')
    expect(svg?.getAttribute('width')).toBe('48')
    expect(svg?.getAttribute('height')).toBe('48')
  })

  it('renders two circles (background track + foreground arc)', () => {
    const { container } = render(<RingChart color="#3b82f6" label="CPU" value={50} />)
    const circles = container.querySelectorAll('circle')
    expect(circles.length).toBe(2)
  })

  it('applies color to foreground circle stroke', () => {
    const { container } = render(<RingChart color="var(--color-chart-1)" label="CPU" value={50} />)
    const circles = container.querySelectorAll('circle')
    const foreground = circles[1]
    expect(foreground.getAttribute('stroke')).toBe('var(--color-chart-1)')
  })
})
