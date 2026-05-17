import { render, screen } from '@testing-library/react'
import type { ReactElement } from 'react'
import { describe, expect, it, vi } from 'vitest'

const navigateMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useNavigate: () => navigateMock
}))

const authState = {
  user: { user_id: '1', username: 'admin', role: 'admin', must_change_password: true },
  isLoading: false,
  isAuthenticated: true
}
vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => authState
}))

vi.mock('@tanstack/react-query', () => ({
  useQueryClient: () => ({ invalidateQueries: vi.fn().mockResolvedValue(undefined) })
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

import { Route } from '../onboarding'

const OnboardingPage = (Route as unknown as { component: () => ReactElement }).component

describe('OnboardingPage', () => {
  it('renders the forced-change form for a flagged user', () => {
    render(<OnboardingPage />)
    expect(screen.getByRole('button', { name: /submit|saving/i })).toBeInTheDocument()
  })

  it('redirects away when the user does not require a password change', () => {
    authState.user = {
      user_id: '1',
      username: 'admin',
      role: 'admin',
      must_change_password: false
    } as typeof authState.user
    render(<OnboardingPage />)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/' })
  })
})
