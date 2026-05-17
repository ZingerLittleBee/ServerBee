import { render, screen } from '@testing-library/react'
import type { ReactElement } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const SUBMIT_BUTTON_PATTERN = /submit|saving/i

const navigateMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useNavigate: () => navigateMock
}))

const FLAGGED_USER = {
  user_id: '1',
  username: 'admin',
  role: 'admin',
  must_change_password: true
}

const authState: {
  user: typeof FLAGGED_USER
  isLoading: boolean
  isAuthenticated: boolean
} = {
  user: { ...FLAGGED_USER },
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

beforeEach(() => {
  navigateMock.mockClear()
  authState.user = { ...FLAGGED_USER }
  authState.isLoading = false
  authState.isAuthenticated = true
})

describe('OnboardingPage', () => {
  it('renders the forced-change form for a flagged user', () => {
    render(<OnboardingPage />)
    expect(screen.getByRole('button', { name: SUBMIT_BUTTON_PATTERN })).toBeInTheDocument()
  })

  it('redirects away when the user does not require a password change', () => {
    authState.user = { ...FLAGGED_USER, must_change_password: false }
    render(<OnboardingPage />)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/' })
  })
})
