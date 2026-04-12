import { describe, expect, it } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { layoutToPatch, mergeLayoutPatch, normalizeNewWidgetPlacement, widgetsToLayout } from './dashboard-layout'

const widgets: DashboardWidget[] = [
  {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: 'stat-number',
    title: 'CPU',
    config_json: '{"metric":"avg_cpu"}',
    grid_x: 0,
    grid_y: 0,
    grid_w: 2,
    grid_h: 1,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
  },
  {
    id: 'w-2',
    dashboard_id: 'dash-1',
    widget_type: 'server-cards',
    title: 'Server Cards',
    config_json: '{"metric":"cpu"}',
    grid_x: 2,
    grid_y: 0,
    grid_w: 3,
    grid_h: 6,
    sort_order: 1,
    created_at: '2026-03-20T00:00:00Z'
  }
]

describe('dashboard-layout', () => {
  it('widgetsToLayout adds min and max constraints from widget definitions', () => {
    const layout = widgetsToLayout(widgets)
    expect(layout[0]).toMatchObject({ i: 'w-1', x: 0, y: 0, w: 2, h: 1, minW: 2, minH: 1, maxW: 2, maxH: 1 })
    // server-cards has no maxW/maxH — stored w=3 is clamped to minW=4
    expect(layout[1]).toMatchObject({ i: 'w-2', x: 2, y: 0, w: 4, h: 6, minW: 4, minH: 3 })
    expect(layout[1]).not.toHaveProperty('maxW')
    expect(layout[1]).not.toHaveProperty('maxH')
  })

  it('widgetsToLayout marks widgets with is_static in config_json as static', () => {
    const staticWidgets: DashboardWidget[] = [
      { ...widgets[0], config_json: '{"metric":"avg_cpu","is_static":true}' },
      { ...widgets[1] }
    ]
    const layout = widgetsToLayout(staticWidgets)
    expect(layout[0].static).toBe(true)
    expect(layout[1].static).toBeUndefined()
  })

  it('layoutToPatch only returns changed widgets', () => {
    const patch = layoutToPatch(
      [
        { i: 'w-1', x: 1, y: 0, w: 2, h: 1 },
        { i: 'w-2', x: 2, y: 0, w: 3, h: 6 }
      ],
      widgets
    )

    expect(patch).toEqual([{ id: 'w-1', grid_x: 1, grid_y: 0, grid_w: 2, grid_h: 1 }])
  })

  it('mergeLayoutPatch only updates layout fields', () => {
    const updated = mergeLayoutPatch(widgets, [{ id: 'w-2', grid_x: 4, grid_y: 1, grid_w: 3, grid_h: 4 }])
    expect(updated[1]).toMatchObject({
      id: 'w-2',
      title: 'Server Cards',
      config_json: '{"metric":"cpu"}',
      sort_order: 1,
      grid_x: 4,
      grid_y: 1,
      grid_w: 3,
      grid_h: 4
    })
  })

  it('normalizeNewWidgetPlacement keeps safe defaults for newly added widgets', () => {
    const newWidget = {
      ...widgets[0],
      id: 'temp-1',
      title: null,
      grid_x: 0,
      grid_y: Number.POSITIVE_INFINITY,
      grid_w: 4,
      grid_h: 3,
      sort_order: 2
    }

    const normalized = normalizeNewWidgetPlacement(widgets, newWidget)
    expect(normalized.at(-1)).toMatchObject({
      id: 'temp-1',
      grid_x: 0,
      grid_y: 6,
      grid_w: 4,
      grid_h: 3,
      sort_order: widgets.length
    })
  })
})
