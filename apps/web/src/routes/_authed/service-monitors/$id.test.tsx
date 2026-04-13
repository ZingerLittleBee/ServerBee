import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <a href="/">{children}</a>,
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useParams: () => ({ id: 'monitor-1' })
  })
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: () => ({
    isPending: false,
    mutate: vi.fn()
  }),
  useQuery: ({ queryKey }: { queryKey: unknown[] }) => {
    if (queryKey[0] === 'service-monitor') {
      return {
        data: {
          config_json: '{}',
          consecutive_failures: 1,
          created_at: '2026-04-14T00:00:00Z',
          enabled: true,
          id: 'monitor-1',
          interval: 60,
          last_checked_at: '2026-04-14T00:00:00Z',
          last_status: false,
          latest_record: {
            detail_json: 'null',
            error: 'TLS handshake failed',
            id: 1,
            latency: null,
            monitor_id: 'monitor-1',
            success: false,
            time: '2026-04-14T00:00:00Z'
          },
          monitor_type: 'ssl',
          name: 'Test SSL Monitor',
          notification_group_id: null,
          retry_count: 1,
          server_ids_json: null,
          target: 'example.com:443',
          updated_at: '2026-04-14T00:00:00Z'
        },
        isLoading: false
      }
    }

    if (queryKey[0] === 'service-monitor-records') {
      return {
        data: [
          {
            detail_json: 'null',
            error: 'TLS handshake failed',
            id: 1,
            latency: null,
            monitor_id: 'monitor-1',
            success: false,
            time: '2026-04-14T00:00:00Z'
          }
        ]
      }
    }

    return { data: undefined, isLoading: false }
  },
  useQueryClient: () => ({
    invalidateQueries: vi.fn().mockResolvedValue(undefined)
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('recharts', () => ({
  Area: () => null,
  AreaChart: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  CartesianGrid: () => null,
  XAxis: () => null,
  YAxis: () => null
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/components/ui/badge', () => ({
  Badge: ({ children }: { children?: ReactNode }) => <span>{children}</span>
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/card', () => ({
  Card: ({ children }: { children?: ReactNode }) => <section>{children}</section>,
  CardContent: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  CardHeader: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  CardTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>
}))

vi.mock('@/components/ui/chart', () => ({
  ChartContainer: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  ChartTooltip: () => null,
  ChartTooltipContent: () => null
}))

vi.mock('@/components/ui/skeleton', () => ({
  Skeleton: (props: Record<string, unknown>) => <div {...props} />
}))

vi.mock('@/components/ui/table', () => ({
  Table: ({ children }: { children?: ReactNode }) => <table>{children}</table>,
  TableBody: ({ children }: { children?: ReactNode }) => <tbody>{children}</tbody>,
  TableCell: ({ children }: { children?: ReactNode }) => <td>{children}</td>,
  TableHead: ({ children }: { children?: ReactNode }) => <th>{children}</th>,
  TableHeader: ({ children }: { children?: ReactNode }) => <thead>{children}</thead>,
  TableRow: ({ children }: { children?: ReactNode }) => <tr>{children}</tr>
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn()
  }
}))

const { ServiceMonitorDetailPage } = await import('./$id')

describe('ServiceMonitorDetailPage', () => {
  it('renders when the latest record detail payload is null JSON', () => {
    render(<ServiceMonitorDetailPage />)

    expect(screen.getByText('Test SSL Monitor')).toBeInTheDocument()
    expect(screen.getByText('history.title')).toBeInTheDocument()
  })
})
