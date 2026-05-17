import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAuth } from '@/hooks/use-auth'
import { ApiError, api } from '@/lib/api-client'
import type { OnboardingRequest } from '@/lib/api-schema'

export const Route = createFileRoute('/onboarding')({
  component: OnboardingPage
})

function OnboardingPage() {
  const { t } = useTranslation('onboarding')
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { user, isLoading, isAuthenticated } = useAuth()

  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (isLoading) {
      return
    }
    if (!isAuthenticated) {
      navigate({ to: '/login' }).catch(() => {
        // non-critical
      })
      return
    }
    if (user && user.must_change_password !== true) {
      navigate({ to: '/' }).catch(() => {
        // non-critical
      })
    }
  }, [isLoading, isAuthenticated, user, navigate])

  useEffect(() => {
    if (user?.username) {
      setUsername(user.username)
    }
  }, [user?.username])

  if (isLoading || !isAuthenticated || user?.must_change_password !== true) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!password) {
      toast.error(t('password_required'))
      return
    }
    if (password !== confirm) {
      toast.error(t('password_mismatch'))
      return
    }
    setSubmitting(true)
    try {
      const payload: OnboardingRequest = {
        new_password: password,
        new_username: username.trim() === user?.username ? null : username.trim() || null
      }
      await api.post('/api/auth/onboarding', payload)
      await queryClient.invalidateQueries({ queryKey: ['auth', 'me'] })
      await navigate({ to: '/' })
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : t('failed')
      toast.error(msg)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <h1 className="font-bold text-2xl">{t('title')}</h1>
          <p className="mt-1 text-muted-foreground text-sm">{t('subtitle')}</p>
        </div>

        <form className="space-y-4" onSubmit={handleSubmit}>
          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="username">
              {t('username')}
            </label>
            <Input
              autoComplete="username"
              id="username"
              onChange={(e) => setUsername(e.target.value)}
              spellCheck={false}
              type="text"
              value={username}
            />
            <p className="text-muted-foreground text-xs">{t('username_hint')}</p>
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="new-password">
              {t('new_password')}
            </label>
            <Input
              autoComplete="new-password"
              id="new-password"
              onChange={(e) => setPassword(e.target.value)}
              required
              type="password"
              value={password}
            />
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="confirm-password">
              {t('confirm_password')}
            </label>
            <Input
              autoComplete="new-password"
              id="confirm-password"
              onChange={(e) => setConfirm(e.target.value)}
              required
              type="password"
              value={confirm}
            />
          </div>

          <Button className="w-full" disabled={submitting} type="submit">
            {submitting ? t('saving') : t('submit')}
          </Button>
        </form>
      </div>
    </div>
  )
}
