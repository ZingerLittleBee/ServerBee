import { LogOut } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { ThemeToggle } from './theme-toggle'

function LanguageSwitcher() {
  const { i18n } = useTranslation()
  const isZh = (i18n.resolvedLanguage ?? i18n.language).startsWith('zh')
  const toggle = () => i18n.changeLanguage(isZh ? 'en' : 'zh')

  return (
    <Button onClick={toggle} size="icon" variant="ghost">
      {isZh ? 'EN' : '中文'}
    </Button>
  )
}

export function Header() {
  const { user, logout } = useAuth()
  const { t } = useTranslation()

  const handleLogout = async () => {
    try {
      await logout()
    } catch {
      // Redirect happens via auth guard
    }
  }

  return (
    <header className="flex h-14 items-center justify-end gap-2 border-b bg-card px-4">
      <LanguageSwitcher />
      <ThemeToggle />
      {user && (
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-sm">{user.username}</span>
          <Button aria-label={t('log_out')} onClick={handleLogout} size="icon" variant="ghost">
            <LogOut className="size-4" />
          </Button>
        </div>
      )}
    </header>
  )
}
