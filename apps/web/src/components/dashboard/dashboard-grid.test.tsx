import { act, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { DashboardGrid } from './dashboard-grid'

interface MockGridLayoutProps {
  children: ReactNode
  layout: Array<{ h: number; i: string; minH?: number; minW?: number; w: number; x: number; y: number }>
  onDrag?: (...args: unknown[]) => void
  onDragStart?: (...args: unknown[]) => void
  onDragStop?: (...args: unknown[]) => void
  onLayoutChange?: (layout: Array<{ h: number; i: string; w: number; x: number; y: number }>) => void
  onResize?: (...args: unknown[]) => void
  onResizeStart?: (...args: unknown[]) => void
  onResizeStop?: (...args: unknown[]) => void
}

let latestGridLayoutProps: MockGridLayoutProps | undefined

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key
  })
}))

vi.mock('./widget-renderer', () => ({
  WidgetRenderer: ({ widget }: { widget: DashboardWidget }) => (
    <div data-testid={`widget-${widget.id}`}>{widget.widget_type}</div>
  )
}))

vi.mock('react-grid-layout', () => ({
  GridLayout: (props: MockGridLayoutProps) => {
    latestGridLayoutProps = props
    return <div data-testid="grid-layout">{props.children}</div>
  },
  useContainerWidth: () => ({ width: 1200, containerRef: { current: null }, mounted: true })
}))

vi.mock('react-grid-layout/css/styles.css', () => ({}))

const widgets: DashboardWidget[] = [
  {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: 'stat-number',
    title: null,
    config_json: '{"metric":"avg_cpu"}',
    grid_x: 0,
    grid_y: 0,
    grid_w: 2,
    grid_h: 2,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
  },
  {
    id: 'w-2',
    dashboard_id: 'dash-1',
    widget_type: 'gauge',
    title: null,
    config_json: '{"server_id":"s1","metric":"cpu"}',
    grid_x: 2,
    grid_y: 0,
    grid_w: 3,
    grid_h: 3,
    sort_order: 1,
    created_at: '2026-03-20T00:00:00Z'
  }
]

const noop = vi.fn()

function getGridLayoutProps(): MockGridLayoutProps {
  if (!latestGridLayoutProps) {
    throw new Error('GridLayout props were not captured')
  }
  return latestGridLayoutProps
}

describe('DashboardGrid', () => {
  const originalInnerWidth = window.innerWidth

  beforeEach(() => {
    latestGridLayoutProps = undefined
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 })
  })

  afterEach(() => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: originalInnerWidth })
  })

  it('renders widgets in view mode without edit/delete overlays', () => {
    render(
      <DashboardGrid
        isEditing={false}
        onLayoutChange={noop}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    expect(screen.getByTestId('widget-w-1')).toBeInTheDocument()
    expect(screen.getByTestId('widget-w-2')).toBeInTheDocument()
    // No Add Widget button in view mode
    expect(screen.queryByText('Add Widget')).not.toBeInTheDocument()
  })

  it('shows Add Widget button in edit mode', () => {
    const onAddWidget = vi.fn()

    render(
      <DashboardGrid
        isEditing
        onAddWidget={onAddWidget}
        onLayoutChange={noop}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    expect(screen.getByText('Add Widget')).toBeInTheDocument()
  })

  it('renders single-column list on mobile (width < 768)', () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 600 })

    render(
      <DashboardGrid
        isEditing={false}
        onLayoutChange={noop}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    // Mobile renders without GridLayout
    expect(screen.queryByTestId('grid-layout')).not.toBeInTheDocument()
    // Widgets still render
    expect(screen.getByTestId('widget-w-1')).toBeInTheDocument()
    expect(screen.getByTestId('widget-w-2')).toBeInTheDocument()
  })

  it('renders GridLayout on desktop', () => {
    render(
      <DashboardGrid
        isEditing={false}
        onLayoutChange={noop}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    expect(screen.getByTestId('grid-layout')).toBeInTheDocument()
  })

  it('updates liveLayout from library onLayoutChange without notifying the parent', () => {
    const onLayoutChange = vi.fn()

    render(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    const nextLayout = [
      { i: 'w-1', x: 1, y: 2, w: 2, h: 2 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ]

    act(() => {
      getGridLayoutProps().onLayoutChange?.(nextLayout)
    })

    expect(onLayoutChange).not.toHaveBeenCalled()
    expect(getGridLayoutProps().layout).toEqual(nextLayout)
  })

  it('keeps drag-time layout changes internal until commit', () => {
    const onLayoutChange = vi.fn()

    render(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    const dragLayout = [
      { i: 'w-1', x: 1, y: 2, w: 2, h: 2 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ]

    act(() => {
      getGridLayoutProps().onDragStart?.()
      getGridLayoutProps().onDrag?.(dragLayout)
    })

    expect(onLayoutChange).not.toHaveBeenCalled()
    expect(getGridLayoutProps().layout).toEqual(dragLayout)
  })

  it('commits only changed widget patches on drag stop', () => {
    const onLayoutChange = vi.fn()

    render(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    act(() => {
      getGridLayoutProps().onDragStart?.()
      getGridLayoutProps().onDrag?.([
        { i: 'w-1', x: 1, y: 1, w: 2, h: 2 },
        { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
      ])
      getGridLayoutProps().onDragStop?.([
        { i: 'w-1', x: 1, y: 1, w: 2, h: 2 },
        { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
      ])
    })

    expect(onLayoutChange).toHaveBeenCalledTimes(1)
    expect(onLayoutChange).toHaveBeenCalledWith([{ id: 'w-1', grid_x: 1, grid_y: 1, grid_w: 2, grid_h: 2 }])
  })

  it('does not overwrite liveLayout from external widget rerenders while dragging', () => {
    const onLayoutChange = vi.fn()
    const { rerender } = render(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    const dragLayout = [
      { i: 'w-1', x: 5, y: 4, w: 2, h: 2 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ]

    act(() => {
      getGridLayoutProps().onDragStart?.()
      getGridLayoutProps().onDrag?.(dragLayout)
    })

    rerender(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={[{ ...widgets[0], grid_x: 9, grid_y: 9 }, widgets[1]]}
      />
    )

    expect(getGridLayoutProps().layout).toEqual(dragLayout)
    expect(onLayoutChange).not.toHaveBeenCalled()
  })

  it('commits changed widget patches on resize stop from the callback layout argument', () => {
    const onLayoutChange = vi.fn()

    render(
      <DashboardGrid
        isEditing
        onLayoutChange={onLayoutChange}
        onWidgetDelete={noop}
        onWidgetEdit={noop}
        servers={[]}
        widgets={widgets}
      />
    )

    const resizeLayout = [
      { i: 'w-1', x: 0, y: 0, w: 4, h: 3 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ]

    act(() => {
      getGridLayoutProps().onResizeStart?.()
      getGridLayoutProps().onResize?.(resizeLayout)
      getGridLayoutProps().onResizeStop?.(resizeLayout)
    })

    expect(onLayoutChange).toHaveBeenCalledTimes(1)
    expect(onLayoutChange).toHaveBeenCalledWith([{ id: 'w-1', grid_x: 0, grid_y: 0, grid_w: 4, grid_h: 3 }])
  })
})
