import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { RecoveryMergeDialog } from './recovery-merge-dialog'

const mockUseRecoveryCandidates = vi.fn()
const mockStartRecoveryMerge = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key
  })
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn()
  }
}))

vi.mock('@/hooks/use-api', () => ({
  useRecoveryCandidates: (...args: unknown[]) => mockUseRecoveryCandidates(...args),
  startRecoveryMerge: (...args: unknown[]) => mockStartRecoveryMerge(...args)
}))

function Wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })

  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

describe('RecoveryMergeDialog', () => {
  it('renders candidate list', () => {
    mockUseRecoveryCandidates.mockReturnValue({
      data: [{ server_id: 'source-1', name: 'Source', score: 42, reasons: ['same remote address'] }],
      isError: false,
      isLoading: false
    })

    render(
      <RecoveryMergeDialog onOpenChange={vi.fn()} open targetServerId="target-1" />,
      { wrapper: Wrapper }
    )

    expect(screen.getByText('Source')).toBeDefined()
    expect(screen.getByText('same remote address')).toBeDefined()
  })

  it('disables submit until a candidate is selected', () => {
    mockUseRecoveryCandidates.mockReturnValue({
      data: [{ server_id: 'source-1', name: 'Source', score: 42, reasons: ['same remote address'] }],
      isError: false,
      isLoading: false
    })

    render(
      <RecoveryMergeDialog onOpenChange={vi.fn()} open targetServerId="target-1" />,
      { wrapper: Wrapper }
    )

    const button = screen.getByText('Start Recovery').closest('button')
    expect(button?.getAttribute('disabled')).toBe('')

    fireEvent.click(screen.getByText('Source'))
    expect(button?.hasAttribute('disabled')).toBe(false)
  })
})
