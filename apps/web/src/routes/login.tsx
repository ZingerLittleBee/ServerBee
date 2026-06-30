import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAuth } from '@/hooks/use-auth'
import { ApiError } from '@/lib/api-client'
import { OAuthButtons } from './oauth-buttons'

export const Route = createFileRoute('/login')({
  component: LoginPage
})

function LoginPage() {
  const { t } = useTranslation('login')
  const navigate = useNavigate()
  const { login, loginError, isLoggingIn } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [totpCode, setTotpCode] = useState('')
  const [needs2FA, setNeeds2FA] = useState(false)

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    try {
      await login({
        username,
        password,
        ...(needs2FA ? { totp_code: totpCode } : {})
      })
      await navigate({ to: '/' })
    } catch (err) {
      if (err instanceof ApiError && err.message.includes('2fa_required')) {
        setNeeds2FA(true)
      } else {
        toast.error(err instanceof Error ? err.message : t('login_failed'))
      }
    }
  }

  const errorMessage = (() => {
    if (!loginError) {
      return null
    }
    if (loginError.message.includes('2fa_required')) {
      return null
    }
    // Parse JSON error response to extract user-friendly message
    try {
      const parsed = JSON.parse(loginError.message)
      if (parsed?.error?.message) {
        return parsed.error.message
      }
    } catch {
      // Not JSON, use as-is
    }
    return loginError.message || t('login_failed')
  })()

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <h1 className="font-bold text-2xl">{t('title')}</h1>
          <p className="mt-1 text-muted-foreground text-sm">{t('subtitle')}</p>
        </div>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {errorMessage && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm">{errorMessage}</div>
          )}

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="username">
              {t('username')}
            </label>
            <Input
              autoComplete="username"
              disabled={needs2FA}
              id="username"
              onChange={(e) => setUsername(e.target.value)}
              placeholder={t('username_placeholder')}
              required
              spellCheck={false}
              type="text"
              value={username}
            />
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="password">
              {t('password')}
            </label>
            <Input
              autoComplete="current-password"
              disabled={needs2FA}
              id="password"
              onChange={(e) => setPassword(e.target.value)}
              required
              type="password"
              value={password}
            />
          </div>

          {needs2FA && (
            <div className="space-y-2">
              <label className="font-medium text-sm" htmlFor="totp">
                {t('two_factor_code')}
              </label>
              <Input
                autoComplete="one-time-code"
                className="font-mono tracking-widest"
                id="totp"
                inputMode="numeric"
                maxLength={6}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                pattern="[0-9]{6}"
                placeholder="000000"
                required
                value={totpCode}
              />
              <p className="text-muted-foreground text-xs">{t('two_factor_hint')}</p>
            </div>
          )}

          <Button className="w-full" disabled={isLoggingIn} type="submit">
            {isLoggingIn ? t('signing_in') : t('sign_in')}
          </Button>
        </form>

        <OAuthButtons />
      </div>
    </div>
  )
}
