import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { DashboardGrid } from './dashboard-grid'

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
  GridLayout: ({ children }: { children: ReactNode }) => <div data-testid="grid-layout">{children}</div>,
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

describe('DashboardGrid', () => {
  const originalInnerWidth = window.innerWidth

  beforeEach(() => {
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
})
