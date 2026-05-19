import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AppSidebar } from './app-sidebar'

const logout = vi.fn().mockResolvedValue(undefined)
const navigate = vi.fn()
const logoutLabel = /logout/i

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <a href="/">{children}</a>,
  useNavigate: () => navigate,
  useMatchRoute: () => () => false
}))

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({ user: { username: 'admin', role: 'admin' }, logout })
}))

vi.mock('@/components/ui/avatar', () => ({
  Avatar: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
  AvatarFallback: ({ children }: { children?: ReactNode }) => <div>{children}</div>
}))

vi.mock('@/components/ui/sidebar', () => {
  const Pass = ({ children }: { children?: ReactNode }) => <div>{children}</div>
  return {
    Sidebar: Pass,
    SidebarContent: Pass,
    SidebarFooter: Pass,
    SidebarGroup: Pass,
    SidebarGroupLabel: Pass,
    SidebarHeader: Pass,
    SidebarMenu: Pass,
    SidebarMenuButton: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
    SidebarMenuItem: Pass,
    useSidebar: () => ({ isMobile: false })
  }
})

// Faithfully mirrors the Base UI `Menu.Item` contract used by the real
// dropdown-menu: the action runs via `onClick`. `onSelect` (a Radix-only
// prop) is intentionally NOT wired, exactly like Base UI ignores it.
vi.mock('@/components/ui/dropdown-menu', () => {
  const Pass = ({ children }: { children?: ReactNode }) => <div>{children}</div>
  return {
    DropdownMenu: Pass,
    DropdownMenuTrigger: Pass,
    DropdownMenuContent: Pass,
    DropdownMenuGroup: Pass,
    DropdownMenuLabel: Pass,
    DropdownMenuSeparator: () => <hr />,
    DropdownMenuItem: ({ children, onClick }: { children?: ReactNode; onClick?: () => void }) => (
      <button onClick={onClick} type="button">
        {children}
      </button>
    )
  }
})

afterEach(() => {
  vi.clearAllMocks()
})

describe('AppSidebar logout', () => {
  it('logs out and navigates to /login when the logout item is activated', async () => {
    render(<AppSidebar />)

    fireEvent.click(screen.getByRole('button', { name: logoutLabel }))

    await vi.waitFor(() => expect(logout).toHaveBeenCalledTimes(1))
    expect(navigate).toHaveBeenCalledWith({ to: '/login' })
  })
})
