import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { ServerIpQualityData, UnlockService } from '@/lib/ip-quality-types'
import { UnlockMatrix } from './unlock-matrix'

const services: UnlockService[] = [
  {
    id: 'svc-netflix',
    key: 'netflix',
    name: 'Netflix',
    category: 'streaming',
    popularity: 100,
    is_builtin: true,
    enabled: true,
    detector: 'netflix',
    request: null,
    rules: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z'
  },
  {
    id: 'svc-hbo',
    key: 'hbo_max',
    name: 'HBO Max',
    category: 'streaming',
    popularity: 70,
    is_builtin: true,
    enabled: true,
    detector: 'hbo_max',
    request: null,
    rules: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z'
  },
  {
    id: 'svc-chatgpt',
    key: 'chatgpt',
    name: 'ChatGPT',
    category: 'ai',
    popularity: 100,
    is_builtin: true,
    enabled: true,
    detector: 'chatgpt',
    request: null,
    rules: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z'
  }
]

const servers: { id: string; name: string }[] = [{ id: 'srv-1', name: 'Tokyo' }]

const overview: ServerIpQualityData[] = [
  {
    server_id: 'srv-1',
    unlock_results: [
      {
        id: 'r-1',
        server_id: 'srv-1',
        service_id: 'svc-netflix',
        status: 'unlocked',
        region: 'JP',
        latency_ms: 120,
        detail: null,
        checked_at: '2026-01-01T00:00:00Z'
      }
    ],
    ip_quality: null
  }
]

describe('UnlockMatrix', () => {
  it('renders one column header per service', () => {
    render(<UnlockMatrix overview={overview} servers={servers} services={services} />)
    expect(screen.getByText('Netflix')).toBeInTheDocument()
    expect(screen.getByText('HBO Max')).toBeInTheDocument()
    expect(screen.getByText('ChatGPT')).toBeInTheDocument()
  })

  it('groups columns by category', () => {
    render(<UnlockMatrix overview={overview} servers={servers} services={services} />)
    const groups = screen.getAllByTestId('matrix-category-group')
    const labels = groups.map((g) => g.getAttribute('data-category'))
    expect(labels).toContain('streaming')
    expect(labels).toContain('ai')
  })

  it('sorts services within a category by popularity descending', () => {
    render(<UnlockMatrix overview={overview} servers={servers} services={services} />)
    const streamingHeaders = screen
      .getAllByTestId('matrix-service-header')
      .filter((h) => h.getAttribute('data-category') === 'streaming')
      .map((h) => h.getAttribute('data-service-key'))
    // Netflix (popularity 100) must come before HBO Max (popularity 70)
    expect(streamingHeaders).toEqual(['netflix', 'hbo_max'])
  })

  it('orders category groups before service columns of the next category', () => {
    render(<UnlockMatrix overview={overview} servers={servers} services={services} />)
    const categories = screen.getAllByTestId('matrix-service-header').map((h) => h.getAttribute('data-category'))
    // streaming columns come first as a contiguous block, then ai
    expect(categories).toEqual(['streaming', 'streaming', 'ai'])
  })

  it('renders a row per server with status cells', () => {
    render(<UnlockMatrix overview={overview} servers={servers} services={services} />)
    const row = screen.getByTestId('matrix-row-srv-1')
    expect(within(row).getByText('Tokyo')).toBeInTheDocument()
    // Netflix cell shows the unlocked status badge
    expect(within(row).getByText('Unlocked')).toBeInTheDocument()
  })

  it('renders an empty state when there are no services', () => {
    render(<UnlockMatrix overview={[]} servers={servers} services={[]} />)
    expect(screen.getByTestId('matrix-empty')).toBeInTheDocument()
  })
})
