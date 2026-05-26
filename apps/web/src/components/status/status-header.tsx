import { Link, useLocation } from '@tanstack/react-router'
import { Server } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { ThemeToggle } from '@/components/layout/theme-toggle'
import { Button } from '@/components/ui/button'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { cn } from '@/lib/utils'

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

function NavLink({ to, label }: { to: string; label: string }) {
  const location = useLocation()
  const active = location.pathname === to || location.pathname.startsWith(`${to}/`)
  return (
    <Link
      className={cn(
        'text-sm transition-colors hover:text-foreground',
        active ? 'font-medium text-foreground underline underline-offset-4' : 'text-muted-foreground'
      )}
      to={to}
    >
      {label}
    </Link>
  )
}

export function StatusHeader() {
  const { t } = useTranslation('status')
  const { data: config } = usePublicStatusConfig()

  return (
    <header className="border-b">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-3">
        <Link className="flex items-center gap-2" to="/status">
          <Server className="size-5 text-primary" />
          <span className="font-semibold text-lg">{config?.title ?? 'Status'}</span>
        </Link>

        <nav className="flex flex-1 items-center justify-center gap-6">
          <NavLink label={t('nav_servers')} to="/status" />
          {config?.show_network && <NavLink label={t('nav_network')} to="/status/network" />}
          {config?.show_ip_quality && <NavLink label={t('nav_ip_quality')} to="/status/ip-quality" />}
        </nav>

        <div className="flex items-center gap-2">
          <LanguageSwitcher />
          <ThemeToggle />
          <Link className="text-muted-foreground text-sm transition-colors hover:text-foreground" to="/login">
            {t('signin')}
          </Link>
        </div>
      </div>
    </header>
  )
}
