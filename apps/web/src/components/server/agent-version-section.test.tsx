import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentVersionSection } from './agent-version-section'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

const mockTriggerUpgrade = vi.fn()
const mockUseUpgradeJob = vi.fn()
vi.mock('@/hooks/use-upgrade-job', () => ({
  useUpgradeJob: (serverId: string) => mockUseUpgradeJob(serverId)
}))

const mockUseAuth = vi.fn()
vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => mockUseAuth()
}))

const mockGetEffectiveCapabilityEnabled = vi.fn()
vi.mock('@/lib/capabilities', () => ({
  CAP_UPGRADE: 4,
  getEffectiveCapabilityEnabled: (...args: unknown[]) => mockGetEffectiveCapabilityEnabled(...args)
}))
const UPGRADE_LATEST_PATTERN = /upgrade_latest_version/
const UPGRADE_ERROR_WITH_BACKUP_PATTERN = /upgrade_error_with_backup/
const UPGRADE_BACKUP_PATH_PATTERN = /upgrade_backup_path/

describe('AgentVersionSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockUseAuth.mockReturnValue({ user: { role: 'admin' } })
    mockUseUpgradeJob.mockReturnValue({
      job: null,
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: false
    })
    mockGetEffectiveCapabilityEnabled.mockReturnValue(true)
  })

  it('renders current agent version', () => {
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.2.3"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('v1.2.3')).toBeDefined()
  })

  it('shows unknown version when agentVersion is null', () => {
    render(
      <AgentVersionSection
        agentVersion={null}
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.2.3"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('vunknown')).toBeDefined()
  })

  it('shows update available badge when versions differ', () => {
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText(UPGRADE_LATEST_PATTERN)).toBeDefined()
  })

  it('shows upgrade button for admin when update available and capability enabled', () => {
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('upgrade_start')).toBeDefined()
  })

  it('does not show upgrade button for non-admin users', () => {
    mockUseAuth.mockReturnValue({ user: { role: 'member' } })
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.queryByText('upgrade_start')).toBeNull()
  })

  it('does not show upgrade button when capability is disabled', () => {
    mockGetEffectiveCapabilityEnabled.mockReturnValue(false)
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={0}
        effectiveCapabilities={0}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.queryByText('upgrade_start')).toBeNull()
  })

  it('shows disabled message for admin when capability is disabled', () => {
    mockGetEffectiveCapabilityEnabled.mockReturnValue(false)
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={0}
        effectiveCapabilities={0}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('cap_disabled')).toBeDefined()
  })

  it('triggers upgrade when button clicked', () => {
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    const button = screen.getByText('upgrade_start')
    fireEvent.click(button)
    expect(mockTriggerUpgrade).toHaveBeenCalledWith('1.3.0')
  })

  it('shows stepper when upgrade is running', () => {
    mockUseUpgradeJob.mockReturnValue({
      job: {
        backup_path: null,
        error: null,
        finished_at: null,
        job_id: 'job-1',
        server_id: 'srv-1',
        stage: 'downloading',
        started_at: new Date().toISOString(),
        status: 'running',
        target_version: '1.3.0'
      },
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: false
    })
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('upgrade_in_progress')).toBeDefined()
    // The stage name appears in both the badge and stepper, so check for multiple occurrences
    const stageElements = screen.getAllByText('upgrade_stage_downloading')
    expect(stageElements.length).toBeGreaterThanOrEqual(1)
  })

  it('shows success state when upgrade succeeded', () => {
    mockUseUpgradeJob.mockReturnValue({
      job: {
        backup_path: null,
        error: null,
        finished_at: new Date().toISOString(),
        job_id: 'job-1',
        server_id: 'srv-1',
        stage: 'restarting',
        started_at: new Date().toISOString(),
        status: 'succeeded',
        target_version: '1.3.0'
      },
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: false
    })
    render(
      <AgentVersionSection
        agentVersion="1.3.0"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('upgrade_status_succeeded')).toBeDefined()
  })

  it('shows failed state with error message', () => {
    mockUseUpgradeJob.mockReturnValue({
      job: {
        backup_path: '/tmp/backup',
        error: 'Download failed: connection timeout',
        finished_at: new Date().toISOString(),
        job_id: 'job-1',
        server_id: 'srv-1',
        stage: 'downloading',
        started_at: new Date().toISOString(),
        status: 'failed',
        target_version: '1.3.0'
      },
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: false
    })
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('upgrade_status_failed')).toBeDefined()
    expect(screen.getByText('Download failed: connection timeout')).toBeDefined()
    expect(screen.getByText(UPGRADE_ERROR_WITH_BACKUP_PATTERN)).toBeDefined()
  })

  it('shows timeout state with backup path', () => {
    mockUseUpgradeJob.mockReturnValue({
      job: {
        backup_path: '/opt/serverbee/backups/agent.bak',
        error: null,
        finished_at: new Date().toISOString(),
        job_id: 'job-1',
        server_id: 'srv-1',
        stage: 'installing',
        started_at: new Date().toISOString(),
        status: 'timeout',
        target_version: '1.3.0'
      },
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: false
    })
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    expect(screen.getByText('upgrade_status_timeout')).toBeDefined()
    expect(screen.getByText(UPGRADE_BACKUP_PATH_PATTERN)).toBeDefined()
  })

  it('disables upgrade button while loading', () => {
    mockUseUpgradeJob.mockReturnValue({
      job: null,
      triggerUpgrade: mockTriggerUpgrade,
      isLoading: true
    })
    render(
      <AgentVersionSection
        agentVersion="1.2.3"
        configuredCapabilities={255}
        effectiveCapabilities={255}
        latestVersion="1.3.0"
        serverId="srv-1"
      />
    )
    const button = screen.getByRole('button')
    expect(button).toBeDisabled()
  })
})
