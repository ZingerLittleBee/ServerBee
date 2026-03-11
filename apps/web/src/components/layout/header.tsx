import { LogOut } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { ThemeToggle } from './theme-toggle'

export function Header() {
  const { user, logout } = useAuth()

  const handleLogout = async () => {
    try {
      await logout()
    } catch {
      // Redirect happens via auth guard
    }
  }

  return (
    <header className="flex h-14 items-center justify-end gap-2 border-b bg-card px-4">
      <ThemeToggle />
      {user && (
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-sm">{user.username}</span>
          <Button aria-label="Log out" onClick={handleLogout} size="icon" variant="ghost">
            <LogOut className="size-4" />
          </Button>
        </div>
      )}
    </header>
  )
}
