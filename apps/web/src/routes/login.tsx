import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { type FormEvent, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { useAuth } from '@/hooks/use-auth'
import { ApiError } from '@/lib/api-client'
import { OAuthButtons } from './oauth-buttons'

const ERROR_BOX_ID = 'login-error'
const HTTP_UNAUTHORIZED = 401
const HTTP_TOO_MANY_REQUESTS = 429

function isTwoFactorRequired(error: unknown): boolean {
  return error instanceof ApiError && error.message.includes('2fa_required')
}

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
  const usernameRef = useRef<HTMLInputElement>(null)
  const totpRef = useRef<HTMLInputElement>(null)

  // The 2FA field only mounts after the first round trip, so move focus once it
  // appears instead of making the user hunt for it.
  useEffect(() => {
    if (needs2FA) {
      totpRef.current?.focus()
    }
  }, [needs2FA])

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
      if (isTwoFactorRequired(err)) {
        setNeeds2FA(true)
        return
      }
      // Other failures surface through the inline error box below; send focus
      // back to the first field the user can actually correct.
      if (needs2FA) {
        totpRef.current?.focus()
      } else {
        usernameRef.current?.focus()
      }
    }
  }

  // Backend errors carry English-only `AppError` messages, so map the status
  // onto localized copy rather than passing the raw text through to the UI.
  const errorMessage = (() => {
    if (!loginError || isTwoFactorRequired(loginError)) {
      return null
    }
    if (loginError instanceof ApiError) {
      if (loginError.status === HTTP_UNAUTHORIZED) {
        return t('unauthorized')
      }
      if (loginError.status === HTTP_TOO_MANY_REQUESTS) {
        return t('rate_limited')
      }
    }
    return t('login_failed')
  })()

  return (
    <ScrollArea className="h-full">
      <div className="flex min-h-dvh items-center justify-center p-4">
        <div className="w-full max-w-sm space-y-6">
          <div className="text-center">
            <h1 className="font-bold text-2xl">{t('title')}</h1>
            <p className="mt-1 text-muted-foreground text-sm">{t('subtitle')}</p>
          </div>

          <form className="space-y-4" onSubmit={handleSubmit}>
            {errorMessage && (
              <div
                className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm"
                id={ERROR_BOX_ID}
                role="alert"
              >
                {errorMessage}
              </div>
            )}

            <div className="space-y-2">
              <label className="font-medium text-sm" htmlFor="username">
                {t('username')}
              </label>
              <Input
                aria-describedby={errorMessage ? ERROR_BOX_ID : undefined}
                aria-invalid={!!errorMessage}
                autoComplete="username"
                disabled={needs2FA}
                id="username"
                onChange={(e) => setUsername(e.target.value)}
                placeholder={t('username_placeholder')}
                ref={usernameRef}
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
                aria-describedby={errorMessage ? ERROR_BOX_ID : undefined}
                aria-invalid={!!errorMessage}
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
                  aria-describedby={errorMessage ? ERROR_BOX_ID : undefined}
                  aria-invalid={!!errorMessage}
                  autoComplete="one-time-code"
                  className="font-mono tracking-widest"
                  id="totp"
                  inputMode="numeric"
                  maxLength={6}
                  onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                  pattern="[0-9]{6}"
                  placeholder="000000"
                  ref={totpRef}
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
    </ScrollArea>
  )
}
