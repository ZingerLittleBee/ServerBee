import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Area, AreaChart, XAxis, YAxis } from './recharts-lazy'

// Regression test: recharts chart containers identify their children (Area,
// XAxis, …) by component type. Re-exporting wrapped components (e.g. via
// React.lazy) breaks that introspection and every chart renders an empty SVG
// shell — which jsdom tests that mock recharts can never catch. Render a real
// chart with explicit dimensions (ResponsiveContainer measures 0×0 in jsdom)
// and assert the series actually draws.
describe('recharts-lazy re-exports', () => {
  it('renders a real series path inside AreaChart', () => {
    const data = [
      { time: 'a', value: 1 },
      { time: 'b', value: 3 },
      { time: 'c', value: 2 }
    ]
    const { container } = render(
      <AreaChart data={data} height={200} width={400}>
        <XAxis dataKey="time" />
        <YAxis />
        <Area dataKey="value" isAnimationActive={false} type="monotone" />
      </AreaChart>
    )

    expect(container.querySelector('.recharts-area')).not.toBeNull()
    expect(container.querySelector('path.recharts-curve')).not.toBeNull()
    expect(container.querySelectorAll('.recharts-cartesian-axis').length).toBe(2)
  })
})
