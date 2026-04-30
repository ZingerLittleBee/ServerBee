import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { CapabilitiesDialog } from './capabilities-dialog'

const mockMutate = vi.fn()
const mockInvalidateQueries = vi.fn()
const upgradeWarningPattern = /Agent does not support capability enforcement/
const translationMap: Record<string, string> = {
  cap_dialog_description: 'Control which agent capabilities are enabled for this server.',
  cap_exec: 'Remote Exec',
  cap_file: 'File Manager',
  cap_group_high_risk: 'High Risk Operations',
  cap_group_low_risk: 'Monitoring & Maintenance',
  cap_high_risk: 'High Risk',
  cap_low_risk: 'Low Risk',
  cap_ping_http: 'HTTP Probe',
  cap_ping_icmp: 'ICMP Ping',
  cap_ping_tcp: 'TCP Probe',
  cap_terminal: 'Web Terminal',
  cap_toggles: 'Capability Toggles',
  cap_upgrade: 'Auto Upgrade',
  cap_upgrade_warning: 'Agent does not support capability enforcement — upgrade recommended',
  detail_capabilities: 'Capabilities'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => translationMap[key] ?? options?.defaultValue ?? key
  })
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({
    user: { role: 'admin' }
  })
}))

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-root">{children}</div>,
  DialogBody: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-body">{children}</div>,
  DialogContent: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-content">{children}</div>,
  DialogHeader: ({ children }: { children?: ReactNode }) => <div data-testid="dialog-header">{children}</div>,
  DialogTitle: ({ children }: { children?: ReactNode }) => <h2>{children}</h2>,
  DialogDescription: ({ children }: { children?: ReactNode }) => <p>{children}</p>
}))

vi.mock('@tanstack/react-query', () => ({
  useMutation: () => ({
    mutate: mockMutate,
    isPending: false
  }),
  useQueryClient: () => ({
    invalidateQueries: mockInvalidateQueries
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

describe('CapabilitiesDialog', () => {
  beforeEach(() => {
    mockMutate.mockReset()
    mockInvalidateQueries.mockReset()
  })

  it('opens a capability console dialog from the trigger button for admins', () => {
    render(
      <CapabilitiesDialog
        server={{
          capabilities: 56,
          id: 'srv-1',
          protocol_version: 1
        }}
      />
    )

    expect(screen.getByRole('button', { name: 'Capabilities' })).toBeInTheDocument()
    expect(screen.queryByText('Capability Toggles')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Capabilities' }))

    expect(screen.getByText('Capability Toggles')).toBeInTheDocument()
    expect(screen.getByText('Control which agent capabilities are enabled for this server.')).toBeInTheDocument()
    expect(screen.getByText('High Risk Operations')).toBeInTheDocument()
    expect(screen.getByText('Monitoring & Maintenance')).toBeInTheDocument()
    expect(screen.getByText(upgradeWarningPattern)).toBeInTheDocument()
    expect(screen.getByText('Web Terminal')).toBeInTheDocument()
    expect(screen.getByText('HTTP Probe')).toBeInTheDocument()
  })
})
