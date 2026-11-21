import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { TailnetOverviewResponse, TailscaleAdminConfigResponse } from '@/lib/api-schema'

const mockPut = vi.fn()
const mockPost = vi.fn()
const mockUseAuth = vi.fn()
const mockUseQuery = vi.fn()
const mockInvalidateQueries = vi.fn(() => Promise.resolve())
const mockSetQueryData = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, params, to }: { children?: ReactNode; params?: { id?: string }; to?: string }) => (
    <a href={params?.id ? `/servers/${params.id}` : (to ?? '/')}>{children}</a>
  ),
  createFileRoute: () => (config: Record<string, unknown>) => config
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: ({
    mutationFn,
    onSuccess,
    onError
  }: {
    mutationFn: (vars: unknown) => Promise<unknown>
    onError?: (err: unknown) => void
    onSuccess?: (data: unknown) => void
  }) => ({
    isPending: false,
    mutate: async (vars: unknown) => {
      try {
        const res = await mutationFn(vars)
        onSuccess?.(res)
      } catch (err) {
        onError?.(err)
      }
    }
  }),
  useQuery: (opts: { enabled?: boolean; queryKey: string[] }) => mockUseQuery(opts),
  useQueryClient: () => ({
    invalidateQueries: mockInvalidateQueries,
    setQueryData: mockSetQueryData
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      if (key === 'settings.testSuccess') {
        return `Connected (${opts?.count} devices found)`
      }
      return key
    }
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => mockUseAuth()
}))

vi.mock('@/lib/api-client', () => ({
  api: {
    get: vi.fn(),
    post: (path: string, body: unknown) => mockPost(path, body),
    put: (path: string, body: unknown) => mockPut(path, body)
  }
}))

const { TailscaleSettingsPage } = await import('./tailscale')

const SAMPLE_CONFIG: TailscaleAdminConfigResponse = {
  enabled: true,
  provider: 'tailscale',
  api_base_url: 'https://api.tailscale.com',
  tailnet: 'example.ts.net',
  has_api_key: true,
  preferred_server_url: 'https://serverbee.example.ts.net',
  auth_enabled: true,
  auto_provision_role: 'member'
}

const SAMPLE_OVERVIEW: TailnetOverviewResponse = {
  configured: true,
  provider: 'tailscale',
  tailnet: 'example.ts.net',
  server_tailnet_url: 'https://serverbee.example.ts.net',
  total_devices: 3,
  online_devices: 2,
  linked_servers: 1,
  expiring_keys_count: 1,
  fetched_at: '2026-11-21T00:00:00Z',
  error: null,
  devices: [
    {
      id: 'dev-1',
      name: 'tokyo-vps',
      hostname: 'tokyo-vps',
      os: 'linux',
      addresses: ['100.64.0.10', 'fd7a:115c:a1e0::10'],
      authorized: true,
      online: true,
      last_seen: '2026-11-21T00:00:00Z',
      key_expiry_disabled: true,
      expires: null,
      client_version: '1.76.1',
      tags: ['tag:server'],
      matched_server_id: 'srv-tokyo',
      matched_server_name: 'Tokyo Production'
    },
    {
      id: 'dev-2',
      name: 'admin-macbook',
      hostname: 'admin-macbook',
      os: 'macOS',
      addresses: ['100.64.0.99'],
      authorized: true,
      online: true,
      last_seen: '2026-11-21T00:00:00Z',
      key_expiry_disabled: false,
      expires: '2026-11-23T00:00:00Z',
      client_version: '1.76.1',
      tags: [],
      matched_server_id: null,
      matched_server_name: null
    }
  ]
}

describe('TailscaleSettingsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockUseAuth.mockReturnValue({ user: { role: 'admin' } })
    mockUseQuery.mockImplementation(({ queryKey, enabled }: { enabled?: boolean; queryKey: string[] }) => {
      if (enabled === false) {
        return { data: undefined, isLoading: false, isFetching: false, refetch: vi.fn() }
      }
      if (queryKey[1] === 'config') {
        return { data: SAMPLE_CONFIG, isLoading: false, isFetching: false, refetch: vi.fn() }
      }
      return { data: SAMPLE_OVERVIEW, isLoading: false, isFetching: false, refetch: vi.fn() }
    })
  })

  it('renders admin config form, preserves existing API key when left blank, and tests connection', async () => {
    mockPost.mockResolvedValueOnce({
      success: true,
      device_count: 3,
      message: 'OK'
    })
    mockPut.mockResolvedValueOnce(SAMPLE_CONFIG)

    render(<TailscaleSettingsPage />)

    expect(screen.getByText('settings.adminCardTitle')).toBeInTheDocument()
    const apiKeyInput = screen.getByLabelText('settings.apiKeyLabel') as HTMLInputElement
    expect(apiKeyInput.placeholder).toBe('settings.apiKeyPlaceholderSet')

    fireEvent.click(screen.getByRole('button', { name: 'settings.testConnection' }))
    await waitFor(() =>
      expect(mockPost).toHaveBeenCalledWith(
        '/api/tailscale/test',
        expect.objectContaining({
          provider: 'tailscale',
          api_base_url: 'https://api.tailscale.com',
          tailnet: 'example.ts.net',
          api_key: undefined
        })
      )
    )
    expect(await screen.findByRole('status')).toHaveTextContent('Connected (3 devices found)')

    fireEvent.click(screen.getByRole('button', { name: 'settings.save' }))
    await waitFor(() => expect(mockPut).toHaveBeenCalledTimes(1))
    expect(mockPut).toHaveBeenCalledWith('/api/tailscale/config', {
      enabled: true,
      provider: 'tailscale',
      api_base_url: 'https://api.tailscale.com',
      tailnet: 'example.ts.net',
      api_key: undefined,
      preferred_server_url: 'https://serverbee.example.ts.net',
      auth_enabled: true,
      auto_provision_role: 'member'
    })
  })

  it('renders global Tailnet devices overview for member users without admin config card and links matched servers', () => {
    mockUseAuth.mockReturnValue({ user: { role: 'member' } })

    render(<TailscaleSettingsPage />)

    expect(screen.queryByText('settings.adminCardTitle')).not.toBeInTheDocument()
    expect(screen.getByText('settings.overviewTitle')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('2')).toBeInTheDocument()

    const serverLink = screen.getByRole('link', { name: 'Tokyo Production' })
    expect(serverLink).toHaveAttribute('href', '/servers/srv-tokyo')
    expect(screen.getByText('admin-macbook')).toBeInTheDocument()
    expect(screen.getByText('settings.devices.unlinked')).toBeInTheDocument()
  })
})
