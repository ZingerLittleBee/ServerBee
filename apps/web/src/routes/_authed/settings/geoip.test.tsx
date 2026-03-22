import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockUseQuery = vi.fn()
const mockUseMutation = vi.fn()
const mockUseQueryClient = vi.fn()

vi.mock('@tanstack/react-query', () => ({
  useQuery: mockUseQuery,
  useMutation: mockUseMutation,
  useQueryClient: mockUseQueryClient
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => () => ({})
}))

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() }
}))

beforeEach(() => {
  vi.clearAllMocks()

  mockUseQueryClient.mockReturnValue({ invalidateQueries: vi.fn() })
  mockUseMutation.mockReturnValue({
    isPending: false,
    mutate: vi.fn()
  })
})

const { GeoIpPage } = (await import('./geoip')) as { GeoIpPage: React.FC }

describe('GeoIpPage', () => {
  it('renders the page heading', () => {
    mockUseQuery.mockReturnValue({ data: undefined, isLoading: false })

    render(<GeoIpPage />)

    expect(screen.getByText('GeoIP Database')).toBeInTheDocument()
  })

  it('shows "Not Installed" when status returns installed: false', () => {
    mockUseQuery.mockReturnValue({
      data: { installed: false },
      isLoading: false
    })

    render(<GeoIpPage />)

    expect(screen.getByText('Not Installed')).toBeInTheDocument()
    expect(screen.getByText('Download the GeoIP database to show server locations on the map')).toBeInTheDocument()
  })

  it('shows "Installed" when status returns installed: true with source "downloaded"', () => {
    mockUseQuery.mockReturnValue({
      data: {
        installed: true,
        source: 'downloaded',
        file_size: 5_242_880,
        updated_at: '2026-03-20T00:00:00Z'
      },
      isLoading: false
    })

    render(<GeoIpPage />)

    expect(screen.getByText('Installed')).toBeInTheDocument()
    expect(screen.getByText(/5\.0 MB/)).toBeInTheDocument()
  })

  it('shows skeleton loader when isLoading is true', () => {
    mockUseQuery.mockReturnValue({ data: undefined, isLoading: true })

    const { container } = render(<GeoIpPage />)

    // Skeleton components render with data-slot="skeleton"
    const skeletons = container.querySelectorAll('[data-slot="skeleton"]')
    expect(skeletons.length).toBeGreaterThan(0)
  })

  it('shows "Download" button when not installed', () => {
    mockUseQuery.mockReturnValue({
      data: { installed: false },
      isLoading: false
    })

    render(<GeoIpPage />)

    expect(screen.getByRole('button', { name: /download/i })).toBeInTheDocument()
  })

  it('shows "Update" button when installed with downloaded source', () => {
    mockUseQuery.mockReturnValue({
      data: { installed: true, source: 'downloaded', file_size: 1024 },
      isLoading: false
    })

    render(<GeoIpPage />)

    expect(screen.getByRole('button', { name: /update/i })).toBeInTheDocument()
  })

  it('hides download/update button when source is "custom"', () => {
    mockUseQuery.mockReturnValue({
      data: { installed: true, source: 'custom', file_size: 1024 },
      isLoading: false
    })

    render(<GeoIpPage />)

    expect(screen.getByText('Using custom MMDB file')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /download|update/i })).not.toBeInTheDocument()
  })
})
