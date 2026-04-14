import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockNavigate = vi.fn()
const mockMutate = vi.fn()
const HEADER_TEST_ID_PATTERN = /^header-/

const settingsTranslations = {
  'capabilities.description': 'Control which features each agent is allowed to use.',
  'capabilities.footer_showing': 'Showing {{filtered}} of {{total}} servers',
  'capabilities.no_servers': 'No servers registered yet',
  'capabilities.search': 'Search servers…',
  'capabilities.server': 'Server',
  'capabilities.title': 'Capabilities'
} as const

const serverTranslations = {
  cap_docker: 'Docker Management',
  cap_exec: 'Remote Exec',
  cap_file: 'File Manager',
  cap_high_risk: 'High Risk',
  cap_low_risk: 'Low Risk',
  cap_ping_http: 'HTTP Probe',
  cap_ping_icmp: 'ICMP Ping',
  cap_ping_tcp: 'TCP Probe',
  cap_terminal: 'Web Terminal',
  cap_upgrade: 'Auto Upgrade',
  cap_upgrade_warning: 'Upgrade recommended'
} as const

const mockServers = [
  {
    agent_local_capabilities: 255,
    capabilities: 56,
    effective_capabilities: 56,
    id: 'srv-1',
    name: 'west-monroe-1',
    protocol_version: 2
  }
]

function interpolate(template: string, values?: Record<string, unknown>) {
  return template.replaceAll(/{{(\w+)}}/g, (_, key: string) => String(values?.[key] ?? ''))
}

vi.mock('@tanstack/react-query', () => ({
  useMutation: () => ({
    isPending: false,
    mutate: mockMutate
  }),
  useQuery: () => ({
    data: mockServers,
    isLoading: false
  }),
  useQueryClient: () => ({
    invalidateQueries: vi.fn()
  })
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => ({
    ...config,
    useNavigate: () => mockNavigate,
    useSearch: () => ({ q: '' })
  })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown> & { defaultValue?: string; ns?: string }) => {
      const translations = options?.ns === 'servers' ? serverTranslations : settingsTranslations
      return interpolate(translations[key as keyof typeof translations] ?? options?.defaultValue ?? key, options)
    }
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/components/ui/data-table', async () => {
  const { flexRender } = await import('@tanstack/react-table')

  return {
    DataTable: ({ table }: { table: { getHeaderGroups: () => Array<{ headers: unknown[]; id: string }> } }) => (
      <div data-testid="capabilities-table">
        {table.getHeaderGroups().map((headerGroup) => (
          <div data-testid={`header-group-${headerGroup.id}`} key={headerGroup.id}>
            {headerGroup.headers.map((header) => {
              const typedHeader = header as {
                column: { columnDef: { header: Parameters<typeof flexRender>[0] } }
                getContext: () => Parameters<typeof flexRender>[1]
                id: string
                isPlaceholder: boolean
              }

              if (typedHeader.isPlaceholder) {
                return null
              }

              return (
                <div data-testid={`header-${typedHeader.id}`} key={typedHeader.id}>
                  {flexRender(typedHeader.column.columnDef.header, typedHeader.getContext())}
                </div>
              )
            })}
          </div>
        ))}
      </div>
    ),
    createSelectColumn: () => ({
      cell: () => null,
      enableSorting: false,
      header: () => <span>Select</span>,
      id: 'select',
      meta: { className: 'w-10' }
    })
  }
})

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />
}))

vi.mock('@/components/ui/skeleton', () => ({
  Skeleton: () => <div />
}))

const { CapabilitiesPage } = await import('./capabilities')

describe('CapabilitiesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders labels for file and docker capability columns', () => {
    render(<CapabilitiesPage />)

    expect(screen.getByText('File Manager')).toBeInTheDocument()
    expect(screen.getByText('Docker Management')).toBeInTheDocument()
  })

  it('renders all high-risk capability columns before low-risk columns', () => {
    render(<CapabilitiesPage />)

    const headerGroup = screen.getByTestId('header-group-0')
    const headerTexts = within(headerGroup)
      .getAllByTestId(HEADER_TEST_ID_PATTERN)
      .map((header) => header.textContent?.replaceAll(/\s+/g, '').trim() ?? '')

    expect(headerTexts).toEqual([
      'Select',
      'Server',
      'WebTerminalHighRisk',
      'RemoteExecHighRisk',
      'FileManagerHighRisk',
      'DockerManagementHighRisk',
      'AutoUpgradeLowRisk',
      'ICMPPingLowRisk',
      'TCPProbeLowRisk',
      'HTTPProbeLowRisk'
    ])
  })
})
