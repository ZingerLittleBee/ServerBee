import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'
import { DashboardEditorView } from './dashboard-editor-view'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key
  })
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('./dashboard-switcher', () => ({
  DashboardSwitcher: ({ currentId }: { currentId: string }) => <div data-testid="dashboard-switcher">{currentId}</div>
}))

vi.mock('./dashboard-grid', () => ({
  DashboardGrid: ({
    isEditing,
    onAddWidget,
    onLayoutChange,
    onWidgetDelete,
    onWidgetEdit,
    widgets
  }: {
    isEditing: boolean
    onAddWidget?: () => void
    onLayoutChange: (patch: { grid_h: number; grid_w: number; grid_x: number; grid_y: number; id: string }[]) => void
    onWidgetDelete: (widgetId: string) => void
    onWidgetEdit: (widgetId: string) => void
    widgets: Array<{ id: string }>
  }) => (
    <div data-testid="dashboard-grid">
      <div data-testid="grid-mode">{isEditing ? 'editing' : 'viewing'}</div>
      <div data-testid="grid-widget-ids">{widgets.map((widget) => widget.id).join(',')}</div>
      <button onClick={() => onLayoutChange([{ id: 'w-1', grid_x: 4, grid_y: 1, grid_w: 5, grid_h: 3 }])} type="button">
        commit-layout
      </button>
      <button onClick={() => onWidgetDelete('w-1')} type="button">
        delete-widget
      </button>
      <button onClick={() => onWidgetEdit('w-1')} type="button">
        edit-widget
      </button>
      <button onClick={() => onAddWidget?.()} type="button">
        add-widget
      </button>
    </div>
  )
}))

vi.mock('./widget-picker', () => ({
  WidgetPicker: ({
    onSelect,
    open
  }: {
    onOpenChange: (open: boolean) => void
    onSelect: (widgetType: string) => void
    open: boolean
  }) =>
    open ? (
      <button onClick={() => onSelect('line-chart')} type="button">
        pick-line-chart
      </button>
    ) : null
}))

vi.mock('./widget-config-dialog', () => ({
  WidgetConfigDialog: ({
    onSubmit,
    open,
    widget,
    widgetType
  }: {
    onOpenChange: (open: boolean) => void
    onSubmit: (title: string, configJson: string) => void
    open: boolean
    servers: unknown[]
    widget?: { title: string | null }
    widgetType: string
  }) =>
    open ? (
      <div data-testid="widget-config-dialog">
        <div data-testid="config-widget-type">{widgetType}</div>
        <div data-testid="config-widget-title">{widget?.title ?? 'new-widget'}</div>
        <button
          onClick={() =>
            onSubmit(
              widget ? `${widget.title ?? 'widget'} updated` : `New ${widgetType}`,
              widget ? '{"metric":"avg_mem"}' : '{"metric":"cpu","range":"24"}'
            )
          }
          type="button"
        >
          submit-config
        </button>
      </div>
    ) : null
}))

const dashboards: Dashboard[] = [
  {
    id: 'dash-1',
    name: 'Primary',
    is_default: true,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z',
    updated_at: '2026-03-20T00:00:00Z'
  }
]

const dashboard: DashboardWithWidgets = {
  ...dashboards[0],
  widgets: [
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
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('DashboardEditorView', () => {
  it('saves committed layout changes from the editor draft', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(
      <DashboardEditorView
        dashboard={dashboard}
        dashboards={dashboards}
        isAdmin
        isSaving={false}
        onSave={onSave}
        onSelectDashboard={vi.fn()}
        servers={[]}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'commit-layout' }))
    fireEvent.click(screen.getByRole('button', { name: 'save' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    expect(onSave).toHaveBeenCalledWith([
      {
        id: 'w-1',
        widget_type: 'stat-number',
        title: 'CPU',
        config_json: { metric: 'avg_cpu' },
        grid_x: 4,
        grid_y: 1,
        grid_w: 5,
        grid_h: 3,
        sort_order: 0
      }
    ])
  })

  it('cancel restores server widgets after deleting from the draft', () => {
    render(
      <DashboardEditorView
        dashboard={dashboard}
        dashboards={dashboards}
        isAdmin
        isSaving={false}
        onSave={vi.fn().mockResolvedValue(undefined)}
        onSelectDashboard={vi.fn()}
        servers={[]}
      />
    )

    expect(screen.getByTestId('grid-widget-ids')).toHaveTextContent('w-1')

    fireEvent.click(screen.getByRole('button', { name: 'edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'delete-widget' }))
    expect(screen.getByTestId('grid-widget-ids')).toHaveTextContent('')

    fireEvent.click(screen.getByRole('button', { name: 'cancel' }))

    expect(screen.getByTestId('grid-mode')).toHaveTextContent('viewing')
    expect(screen.getByTestId('grid-widget-ids')).toHaveTextContent('w-1')
  })

  it('adds a widget through the picker and config flow using the editor hook draft', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(
      <DashboardEditorView
        dashboard={dashboard}
        dashboards={dashboards}
        isAdmin
        isSaving={false}
        onSave={onSave}
        onSelectDashboard={vi.fn()}
        servers={[]}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'add-widget' }))
    fireEvent.click(screen.getByRole('button', { name: 'pick-line-chart' }))
    expect(screen.getByTestId('config-widget-type')).toHaveTextContent('line-chart')
    fireEvent.click(screen.getByRole('button', { name: 'submit-config' }))
    fireEvent.click(screen.getByRole('button', { name: 'save' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    expect(onSave).toHaveBeenCalledWith([
      {
        id: 'w-1',
        widget_type: 'stat-number',
        title: 'CPU',
        config_json: { metric: 'avg_cpu' },
        grid_x: 0,
        grid_y: 0,
        grid_w: 2,
        grid_h: 2,
        sort_order: 0
      },
      expect.objectContaining({
        id: undefined,
        widget_type: 'line-chart',
        title: 'New line-chart',
        config_json: { metric: 'cpu', range: '24' },
        grid_x: 0,
        grid_y: 2,
        grid_w: 6,
        grid_h: 4,
        sort_order: 1
      })
    ])
  })
})
