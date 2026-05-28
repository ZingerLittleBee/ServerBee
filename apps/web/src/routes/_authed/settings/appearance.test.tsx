import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

function createMemoryStorage(): Storage {
  const store = new Map<string, string>()

  return {
    get length() {
      return store.size
    },
    clear: () => store.clear(),
    getItem: (key: string) => store.get(key) ?? null,
    key: (index: number) => Array.from(store.keys())[index] ?? null,
    removeItem: (key: string) => {
      store.delete(key)
    },
    setItem: (key: string, value: string) => {
      store.set(key, value)
    }
  }
}

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => config,
  Link: ({ children, to }: { children: ReactNode; to: string }) => <a href={to}>{children}</a>
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: vi.fn().mockResolvedValue({}),
    put: vi.fn().mockResolvedValue(undefined)
  }
}))

const { AppearancePage } = await import('./appearance')

function wrap(node: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })
  return <QueryClientProvider client={queryClient}>{node}</QueryClientProvider>
}

describe('AppearancePage', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', createMemoryStorage())
    vi.stubGlobal('matchMedia', (_query: string) => ({
      matches: false,
      addEventListener: () => undefined,
      removeEventListener: () => undefined
    }))
  })

  it('renders the brand settings section', () => {
    render(wrap(<AppearancePage />))
    expect(screen.getByText('appearance.brand_settings')).toBeInTheDocument()
  })

  it('renders the widget modules notice linking to /settings/widgets', () => {
    render(wrap(<AppearancePage />))
    expect(screen.getByText('appearance.theme_moved_title')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'appearance.theme_moved_cta' })).toHaveAttribute(
      'href',
      '/settings/widgets'
    )
  })
})
