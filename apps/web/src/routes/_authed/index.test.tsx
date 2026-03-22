import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'

const mockUseQuery = vi.fn()
const mockUseAuth = vi.fn()
const mockUseDashboards = vi.fn()
const mockUseDefaultDashboard = vi.fn()
const mockUseDashboard = vi.fn()
const mockUseUpdateDashboard = vi.fn()

vi.mock('@tanstack/react-query', () => ({
  useQuery: mockUseQuery
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => () => ({})
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: mockUseAuth
}))

vi.mock('@/hooks/use-dashboard', () => ({
  useDashboard: mockUseDashboard,
  useDashboards: mockUseDashboards,
  useDefaultDashboard: mockUseDefaultDashboard,
  useUpdateDashboard: mockUseUpdateDashboard
}))

vi.mock('@/components/dashboard/dashboard-editor-view', () => ({
  DashboardEditorView: ({
    activeDashboardId,
    dashboard,
    onSelectDashboard
  }: {
    activeDashboardId: string
    dashboard?: DashboardWithWidgets
    onSelectDashboard: (id: string) => void
  }) => (
    <div>
      <div data-testid="active-dashboard-id">{activeDashboardId || 'none'}</div>
      <div data-testid="loaded-dashboard-id">{dashboard?.id ?? 'none'}</div>
      <button onClick={() => onSelectDashboard('dash-2')} type="button">
        switch-dashboard
      </button>
    </div>
  )
}))

const dashboards: Dashboard[] = [
  {
    id: 'dash-1',
    name: 'Primary',
    is_default: true,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z',
    updated_at: '2026-03-20T00:00:00Z'
  },
  {
    id: 'dash-2',
    name: 'Secondary',
    is_default: false,
    sort_order: 1,
    created_at: '2026-03-20T00:00:00Z',
    updated_at: '2026-03-20T00:00:00Z'
  }
]

const defaultDashboard: DashboardWithWidgets = {
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

  mockUseQuery.mockReturnValue({ data: [] })
  mockUseAuth.mockReturnValue({ user: { role: 'admin' } })
  mockUseDashboards.mockReturnValue({ data: dashboards })
  mockUseDefaultDashboard.mockReturnValue({ data: defaultDashboard })
  mockUseUpdateDashboard.mockReturnValue({
    isPending: false,
    mutateAsync: vi.fn().mockResolvedValue(undefined)
  })
  mockUseDashboard.mockImplementation((id: string) => ({
    data: id === 'dash-1' ? defaultDashboard : undefined
  }))
})

const { DashboardPage } = await import('./index')

describe('DashboardPage', () => {
  it('keeps the selected dashboard id while the next dashboard data is loading', () => {
    render(<DashboardPage />)

    expect(screen.getByTestId('active-dashboard-id')).toHaveTextContent('dash-1')
    expect(screen.getByTestId('loaded-dashboard-id')).toHaveTextContent('dash-1')

    fireEvent.click(screen.getByRole('button', { name: 'switch-dashboard' }))

    expect(screen.getByTestId('active-dashboard-id')).toHaveTextContent('dash-2')
    expect(screen.getByTestId('loaded-dashboard-id')).toHaveTextContent('none')
  })
})
