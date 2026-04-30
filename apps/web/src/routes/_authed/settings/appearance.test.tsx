import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockSetActiveThemeRef = vi.fn()
let mockRole = 'admin'
let mockActiveThemeRef = 'preset:default'

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
  createFileRoute: () => (config: Record<string, unknown>) => config
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

const { LegacyMigrationPrompt } = await import('./appearance')

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
