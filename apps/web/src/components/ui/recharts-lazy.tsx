import { type ComponentType, createElement, lazy } from 'react'

type RechartsModule = typeof import('recharts')
type LazyRechartsComponent = ComponentType<Record<string, unknown>>

function lazyRechartsComponent(name: keyof RechartsModule) {
  return lazy(async () => {
    const mod = await import('recharts')
    const Component = mod[name] as LazyRechartsComponent

    return {
      default: (props: Record<string, unknown>) => createElement(Component, props)
    }
  })
}

export const Area = lazyRechartsComponent('Area')
export const AreaChart = lazyRechartsComponent('AreaChart')
export const Bar = lazyRechartsComponent('Bar')
export const BarChart = lazyRechartsComponent('BarChart')
export const CartesianGrid = lazyRechartsComponent('CartesianGrid')
export const Legend = lazyRechartsComponent('Legend')
export const Line = lazyRechartsComponent('Line')
export const LineChart = lazyRechartsComponent('LineChart')
export const ResponsiveContainer = lazyRechartsComponent('ResponsiveContainer')
export const Tooltip = lazyRechartsComponent('Tooltip')
export const XAxis = lazyRechartsComponent('XAxis')
export const YAxis = lazyRechartsComponent('YAxis')
