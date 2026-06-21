import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockNavigate = vi.fn()
const HEADER_TEST_ID_PATTERN = /^header-/

const settingsTranslations = {
  'capabilities.description': 'Capabilities are configured in each agent config file. This view is read-only.',
  'capabilities.footer_showing': 'Showing {{filtered}} of {{total}} servers',
  'capabilities.no_servers': 'No servers registered yet',
  'capabilities.offline': 'Offline',
  'capabilities.search': 'Search servers…',
  'capabilities.server': 'Server',
  'capabilities.summary': '{{total}} servers · {{online}} online',
  'capabilities.title': 'Capabilities'
} as const

const serverTranslations = {
  cap_disabled: 'Disabled',
  cap_docker: 'Docker Management',
  cap_enabled: 'Enabled',
  cap_exec: 'Remote Exec',
  cap_file: 'File Manager',
  cap_high_risk: 'High Risk',
  cap_medium_risk: 'Medium Risk',
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
    agent_local_capabilities: 56,
    capabilities: 56,
    effective_capabilities: 56,
    id: 'srv-1',
    name: 'west-monroe-1',
    online: true,
    protocol_version: 2
  }
]

function interpolate(template: string, values?: Record<string, unknown>) {
  return template.replaceAll(/{{(\w+)}}/g, (_, key: string) => String(values?.[key] ?? ''))
}

vi.mock('@tanstack/react-query', () => ({
  useQuery: () => ({
    data: mockServers,
    isLoading: false
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
    )
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

  it('orders capability columns by risk tier: high, then medium, then low', () => {
    render(<CapabilitiesPage />)

    const headerGroup = screen.getByTestId('header-group-0')
    const headerTexts = within(headerGroup)
      .getAllByTestId(HEADER_TEST_ID_PATTERN)
      .map((header) => header.textContent?.replaceAll(/\s+/g, '').trim() ?? '')

    expect(headerTexts).toEqual([
      'Server',
      'WebTerminalHighRisk',
      'RemoteExecHighRisk',
      'FileManagerHighRisk',
      'DockerManagementHighRisk',
      'cap_firewall_blockMediumRisk',
      'cap_ip_qualityMediumRisk',
      'AutoUpgradeLowRisk',
      'ICMPPingLowRisk',
      'TCPProbeLowRisk',
      'HTTPProbeLowRisk',
      'cap_security_eventsLowRisk'
    ])
  })
})
