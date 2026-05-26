import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockSetActiveThemeRef = vi.fn()
let mockRole = 'admin'
let mockActiveThemeRef = 'preset:default'
let mockActiveSpaThemeId: string | null = null

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

const mockNavigate = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => mockNavigate
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      key === 'appearance.legacy_theme_migration.detected' ? `Detected ${String(options?.theme ?? '')}` : key
  })
}))

vi.mock('@/components/theme-provider', () => ({
  useTheme: () => ({
    activeTheme: { ref: mockActiveThemeRef, theme: { kind: 'preset', id: 'default' } },
    setActiveThemeRef: mockSetActiveThemeRef
  })
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({
    user: { role: mockRole }
  })
}))

// SPA-theme API mocks. The hooks live in @/api/spa-themes; tests need to
// control useActiveSpaTheme + useSpaThemes return values to drive the
// CustomSpaThemeSection/Banner render paths.
vi.mock('@/api/spa-themes', () => ({
  useActiveSpaTheme: () => ({ data: { theme_id: mockActiveSpaThemeId } }),
  useSpaThemes: () => ({ data: [] }),
  useActivateSpaTheme: () => ({ mutate: vi.fn() }),
  useDeleteSpaTheme: () => ({ mutate: vi.fn() }),
  useUploadSpaTheme: () => ({ mutate: vi.fn(), isPending: false })
}))

// Legacy color-theme APIs used by ThemeGrid; we don't need their behaviour, just
// to keep the page from blowing up during render.
vi.mock('@/api/themes', () => ({
  useCustomThemes: () => ({ data: [] }),
  useDuplicateTheme: () => ({ mutate: vi.fn() }),
  useImportTheme: () => ({ mutate: vi.fn() }),
  useThemeQuery: () => ({ data: undefined })
}))

const { AppearancePage, LegacyMigrationPrompt } = await import('./appearance')

function wrap(node: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })
  return <QueryClientProvider client={queryClient}>{node}</QueryClientProvider>
}

describe('LegacyMigrationPrompt', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', createMemoryStorage())
    mockSetActiveThemeRef.mockReset()
    mockRole = 'admin'
    mockActiveThemeRef = 'preset:default'
  })

  it('lets admins apply the old browser color theme as the global active theme', () => {
    localStorage.setItem('color-theme', 'tokyo-night')

    render(<LegacyMigrationPrompt />)

    expect(screen.getByText('Detected tokyo-night')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'appearance.legacy_theme_migration.apply' }))

    expect(mockSetActiveThemeRef).toHaveBeenCalledWith('preset:tokyo-night')
    expect(localStorage.getItem('theme-migration-prompted')).toBe('1')
    expect(localStorage.getItem('color-theme')).toBeNull()
  })

  it('does not prompt members', () => {
    localStorage.setItem('color-theme', 'tokyo-night')
    mockRole = 'member'

    render(<LegacyMigrationPrompt />)

    expect(screen.queryByText('Detected tokyo-night')).not.toBeInTheDocument()
  })

  it('does not prompt when the active theme is already customized', () => {
    localStorage.setItem('color-theme', 'tokyo-night')
    mockActiveThemeRef = 'preset:nord'

    render(<LegacyMigrationPrompt />)

    expect(screen.queryByText('Detected tokyo-night')).not.toBeInTheDocument()
  })
})

describe('AppearancePage', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', createMemoryStorage())
    // ThemeGrid → useResolvedIsDark calls window.matchMedia; jsdom doesn't provide it.
    vi.stubGlobal('matchMedia', (_query: string) => ({
      matches: false,
      addEventListener: () => undefined,
      removeEventListener: () => undefined
    }))
    mockNavigate.mockReset()
    mockRole = 'admin'
    mockActiveThemeRef = 'preset:default'
    mockActiveSpaThemeId = null
  })

  it('renders the custom SPA theme section for admin users', () => {
    mockRole = 'admin'
    render(wrap(<AppearancePage />))
    expect(screen.getByText('section_title')).toBeInTheDocument()
  })

  it('hides the custom SPA theme section from member users', () => {
    mockRole = 'member'
    render(wrap(<AppearancePage />))
    expect(screen.queryByText('section_title')).not.toBeInTheDocument()
  })

  it('shows the color/brand disabled banner when an SPA theme is active', () => {
    mockActiveSpaThemeId = 'some-uuid'
    render(wrap(<AppearancePage />))
    expect(screen.getByText('color_brand_disabled_banner')).toBeInTheDocument()
  })

  it('hides the banner when no SPA theme is active', () => {
    mockActiveSpaThemeId = null
    render(wrap(<AppearancePage />))
    expect(screen.queryByText('color_brand_disabled_banner')).not.toBeInTheDocument()
  })
})
