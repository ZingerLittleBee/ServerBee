import { renderHook } from '@testing-library/react'
import { act } from 'react'
import { describe, expect, it } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { useDashboardEditor } from './use-dashboard-editor'

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
    grid_h: 2,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
  }
]

describe('useDashboardEditor', () => {
  it('starts editing from a cloned widget draft', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))

    expect(result.current.isEditing).toBe(true)
    expect(result.current.isDirty).toBe(false)
    expect(result.current.draftWidgets).toEqual(widgets)
    expect(result.current.draftWidgets).not.toBe(widgets)
  })

  it('commitLayoutPatch only updates layout fields', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))
    act(() =>
      result.current.commitLayoutPatch([{ id: 'w-1', grid_x: 4, grid_y: 1, grid_w: 3, grid_h: 2 }])
    )

    expect(result.current.draftWidgets[0]).toMatchObject({
      title: 'CPU',
      config_json: '{"metric":"avg_cpu"}',
      grid_x: 4,
      grid_y: 1,
      grid_w: 3,
      grid_h: 2
    })
  })

  it('commitLayoutPatch with an empty patch is a no-op', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))
    act(() => result.current.commitLayoutPatch([]))

    expect(result.current.draftWidgets).toEqual(widgets)
    expect(result.current.isDirty).toBe(false)
  })

  it('updateWidget leaves layout untouched', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))
    act(() => result.current.updateWidget('w-1', { title: 'Memory', config_json: '{"metric":"avg_mem"}' }))

    expect(result.current.draftWidgets[0]).toMatchObject({
      title: 'Memory',
      config_json: '{"metric":"avg_mem"}',
      grid_x: 0,
      grid_y: 0
    })
  })

  it('addWidget creates and places a new draft widget internally', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))
    act(() =>
      result.current.addWidget({
        dashboardId: 'dash-1',
        widgetType: 'line-chart',
        title: 'Memory',
        configJson: '{"metric":"avg_mem"}'
      })
    )

    expect(result.current.draftWidgets).toHaveLength(2)
    expect(result.current.draftWidgets[1]).toMatchObject({
      dashboard_id: 'dash-1',
      id: expect.stringMatching(/^temp-/),
      widget_type: 'line-chart',
      title: 'Memory',
      config_json: '{"metric":"avg_mem"}',
      grid_x: 0,
      grid_y: 2,
      grid_w: 6,
      grid_h: 4,
      sort_order: 1,
      created_at: expect.any(String)
    })
  })

  it('deleteWidget removes the draft widget and reindexes sort order', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing([...widgets, { ...widgets[0], id: 'w-2', sort_order: 1 }]))
    act(() => result.current.deleteWidget('w-1'))

    expect(result.current.draftWidgets).toHaveLength(1)
    expect(result.current.draftWidgets[0]).toMatchObject({
      id: 'w-2',
      sort_order: 0
    })
  })

  it('cancelEditing clears editor state', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing(widgets))
    act(() => result.current.updateWidget('w-1', { title: 'Memory' }))
    expect(result.current.isDirty).toBe(true)

    act(() => result.current.cancelEditing())

    expect(result.current.isEditing).toBe(false)
    expect(result.current.isDirty).toBe(false)
    expect(result.current.draftWidgets).toEqual([])
  })

  it('buildSaveInput parses config_json and preserves layout fields', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing([{ ...widgets[0], id: 'temp-1', sort_order: 0 }]))
    act(() =>
      result.current.updateWidget('temp-1', {
        title: null,
        config_json: '{"metric":"avg_mem","window":5}'
      })
    )

    expect(result.current.buildSaveInput()[0]).toEqual({
      id: undefined,
      widget_type: 'stat-number',
      title: null,
      config_json: { metric: 'avg_mem', window: 5 },
      grid_x: 0,
      grid_y: 0,
      grid_w: 2,
      grid_h: 2,
      sort_order: 0
    })
  })

  it('buildSaveInput falls back to an empty config object for invalid JSON', () => {
    const { result } = renderHook(() => useDashboardEditor())

    act(() => result.current.startEditing([{ ...widgets[0], id: 'temp-1', sort_order: 0, config_json: '{' }]))

    expect(result.current.buildSaveInput()[0].config_json).toEqual({})
  })
})
