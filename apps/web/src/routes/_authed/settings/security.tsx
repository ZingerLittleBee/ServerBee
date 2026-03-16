import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Check, Link2Off, Loader2, Shield, ShieldOff, Smartphone } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { OAuthAccount, TotpSetupResponse, TotpStatusResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/security')({
  component: SecurityPage
})

function SecurityPage() {
  const { t } = useTranslation(['settings', 'common'])
  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('security.title')}</h1>
      <div className="max-w-2xl space-y-8">
        <TwoFactorSection />
        <ChangePasswordSection />
        <OAuthAccountsSection />
      </div>
    </div>
  )
}

function TwoFactorSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [setupData, setSetupData] = useState<TotpSetupResponse | null>(null)
  const [verifyCode, setVerifyCode] = useState('')

  const { data: status, isLoading } = useQuery<TotpStatusResponse>({
    queryKey: ['auth', '2fa', 'status'],
    queryFn: () => api.get<TotpStatusResponse>('/api/auth/2fa/status')
  })

  const setupMutation = useMutation({
    mutationFn: () => api.post<TotpSetupResponse>('/api/auth/2fa/setup'),
    onSuccess: (data) => {
      setSetupData(data)
      toast.success('2FA setup initiated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  const enableMutation = useMutation({
    mutationFn: (code: string) => api.post('/api/auth/2fa/enable', { code }),
    onSuccess: () => {
      setSetupData(null)
      setVerifyCode('')
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
      toast.success('2FA enabled')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  const disableMutation = useMutation({
    mutationFn: (password: string) => api.post('/api/auth/2fa/disable', { password }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
      toast.success('2FA disabled')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  const handleEnable = (e: FormEvent) => {
    e.preventDefault()
    if (!setupData || verifyCode.length !== 6) {
      return
    }
    enableMutation.mutate(verifyCode)
  }

  const [disablePassword, setDisablePassword] = useState('')
  const [showDisable, setShowDisable] = useState(false)

  const handleDisable = (e: FormEvent) => {
    e.preventDefault()
    if (disablePassword.length === 0) {
      return
    }
    disableMutation.mutate(disablePassword)
    setDisablePassword('')
    setShowDisable(false)
  }

  if (isLoading) {
    return (
      <div className="rounded-lg border bg-card p-6">
        <Skeleton className="h-20" />
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-6">
      <div className="mb-4 flex items-center gap-2">
        <Smartphone className="size-5" />
        <h2 className="font-semibold text-lg">{t('security.two_factor')}</h2>
      </div>

      {status?.enabled && (
        <div className="space-y-4">
          <div className="flex items-center gap-2 text-emerald-600 dark:text-emerald-400">
            <Shield className="size-4" />
            <span className="font-medium text-sm">{t('security.two_factor_enabled')}</span>
          </div>

          {showDisable ? (
            <form className="space-y-3" onSubmit={handleDisable}>
              <p className="text-muted-foreground text-sm">{t('security.enter_password_disable')}</p>
              <Input
                autoComplete="current-password"
                className="max-w-xs"
                onChange={(e) => setDisablePassword(e.target.value)}
                placeholder={t('security.current_password')}
                required
                type="password"
                value={disablePassword}
              />
              <div className="flex gap-2">
                <Button disabled={disableMutation.isPending} type="submit" variant="destructive">
                  {t('security.confirm_disable')}
                </Button>
                <Button
                  onClick={() => {
                    setShowDisable(false)
                    setDisablePassword('')
                  }}
                  type="button"
                  variant="outline"
                >
                  {t('common:cancel')}
                </Button>
              </div>
              {disableMutation.error && (
                <p className="text-destructive text-sm">
                  {disableMutation.error.message || t('security.disable_failed')}
                </p>
              )}
            </form>
          ) : (
            <Button onClick={() => setShowDisable(true)} variant="destructive">
              <ShieldOff className="size-4" />
              {t('security.disable_2fa')}
            </Button>
          )}
        </div>
      )}
      {!status?.enabled && setupData && (
        <div className="space-y-4">
          <p className="text-muted-foreground text-sm">{t('security.scan_qr')}</p>

          <div className="flex justify-center rounded-md border bg-white p-4">
            <img
              alt={t('security.qr_alt')}
              height={192}
              src={`data:image/png;base64,${setupData.qr_code_base64}`}
              width={192}
            />
          </div>

          <details className="text-sm">
            <summary className="cursor-pointer text-muted-foreground">{t('security.cant_scan')}</summary>
            <code className="mt-1 block break-all rounded bg-muted px-2 py-1 font-mono text-xs">
              {setupData.secret}
            </code>
          </details>

          <form className="space-y-3" onSubmit={handleEnable}>
            <label className="font-medium text-sm" htmlFor="totp-code">
              {t('security.enter_code')}
            </label>
            <Input
              autoComplete="one-time-code"
              className="max-w-xs font-mono tracking-widest"
              id="totp-code"
              inputMode="numeric"
              maxLength={6}
              onChange={(e) => setVerifyCode(e.target.value.replace(/\D/g, ''))}
              pattern="[0-9]{6}"
              placeholder="000000"
              required
              value={verifyCode}
            />
            <div className="flex gap-2">
              <Button disabled={enableMutation.isPending || verifyCode.length !== 6} type="submit">
                {enableMutation.isPending ? <Loader2 className="size-4 animate-spin" /> : <Check className="size-4" />}
                {t('security.verify_enable')}
              </Button>
              <Button
                onClick={() => {
                  setSetupData(null)
                  setVerifyCode('')
                }}
                type="button"
                variant="outline"
              >
                {t('common:cancel')}
              </Button>
            </div>
            {enableMutation.error && <p className="text-destructive text-sm">{t('security.invalid_code')}</p>}
          </form>
        </div>
      )}
      {!(status?.enabled || setupData) && (
        <div className="space-y-3">
          <p className="text-muted-foreground text-sm">{t('security.two_factor_description')}</p>
          <Button disabled={setupMutation.isPending} onClick={() => setupMutation.mutate()}>
            <Shield className="size-4" />
            {t('security.setup_2fa')}
          </Button>
        </div>
      )}
    </div>
  )
}

function ChangePasswordSection() {
  const { t } = useTranslation(['settings', 'common'])
  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')

  const mutation = useMutation({
    mutationFn: (payload: { new_password: string; old_password: string }) => api.put('/api/auth/password', payload),
    onSuccess: () => {
      setOldPassword('')
      setNewPassword('')
      toast.success('Password changed')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    mutation.mutate({ old_password: oldPassword, new_password: newPassword })
  }

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-4 font-semibold text-lg">{t('security.change_password')}</h2>

      <form className="max-w-xs space-y-3" onSubmit={handleSubmit}>
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="old-pw">
            {t('security.current_password')}
          </label>
          <Input
            autoComplete="current-password"
            id="old-pw"
            onChange={(e) => setOldPassword(e.target.value)}
            required
            type="password"
            value={oldPassword}
          />
        </div>
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="new-pw">
            {t('security.new_password')}
          </label>
          <Input
            autoComplete="new-password"
            id="new-pw"
            minLength={8}
            onChange={(e) => setNewPassword(e.target.value)}
            required
            type="password"
            value={newPassword}
          />
        </div>

        {mutation.error && (
          <p className="text-destructive text-sm">{mutation.error.message || t('security.change_failed')}</p>
        )}

        <Button disabled={mutation.isPending} type="submit">
          {mutation.isPending ? t('security.changing') : t('security.change_password')}
        </Button>
      </form>
    </div>
  )
}

function OAuthAccountsSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()

  const { data: accounts, isLoading } = useQuery<OAuthAccount[]>({
    queryKey: ['auth', 'oauth', 'accounts'],
    queryFn: () => api.get<OAuthAccount[]>('/api/auth/oauth/accounts')
  })

  const unlinkMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/auth/oauth/accounts/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', 'oauth', 'accounts'] }).catch(() => undefined)
      toast.success('Account unlinked')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-4 font-semibold text-lg">{t('security.linked_accounts')}</h2>

      {isLoading && (
        <div className="space-y-2">
          <Skeleton className="h-12" />
          <Skeleton className="h-12" />
        </div>
      )}
      {!isLoading && (!accounts || accounts.length === 0) && (
        <p className="text-muted-foreground text-sm">{t('security.no_linked_accounts')}</p>
      )}
      {!isLoading && accounts && accounts.length > 0 && (
        <div className="space-y-2">
          {accounts.map((acct) => (
            <div className="flex items-center justify-between rounded-md border px-4 py-3" key={acct.id}>
              <div>
                <div className="flex items-center gap-2">
                  <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs uppercase">{acct.provider}</span>
                  <span className="font-medium text-sm">
                    {acct.display_name || acct.email || acct.provider_user_id}
                  </span>
                </div>
                {acct.email && acct.display_name && (
                  <p className="mt-0.5 text-muted-foreground text-xs">{acct.email}</p>
                )}
              </div>
              <Button
                aria-label={`${t('security.unlink')} ${acct.provider}`}
                disabled={unlinkMutation.isPending}
                onClick={() => unlinkMutation.mutate(acct.id)}
                size="sm"
                variant="outline"
              >
                <Link2Off className="size-3.5" />
                {t('security.unlink')}
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
