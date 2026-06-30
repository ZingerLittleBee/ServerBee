import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Loader2, Shield, ShieldOff, Smartphone } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { TotpSetupResponse, TotpStatusResponse } from '@/lib/api-schema'

export function TwoFactorSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [setupData, setSetupData] = useState<TotpSetupResponse | null>(null)
  const [setupPending, setSetupPending] = useState(false)
  const [verifyCode, setVerifyCode] = useState('')

  const { data: status, isLoading } = useQuery<TotpStatusResponse>({
    queryKey: ['auth', '2fa', 'status'],
    queryFn: () => api.get<TotpStatusResponse>('/api/auth/2fa/status')
  })

  const handleSetup = async () => {
    if (setupPending) {
      return
    }
    setSetupPending(true)
    try {
      const data = await api.post<TotpSetupResponse>('/api/auth/2fa/setup')
      setSetupData(data)
      toast.success(t('security.toast_2fa_setup'))
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    } finally {
      setSetupPending(false)
    }
  }

  const enableMutation = useMutation({
    mutationFn: (code: string) => api.post('/api/auth/2fa/enable', { code }),
    onSuccess: () => {
      setSetupData(null)
      setVerifyCode('')
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
      toast.success(t('security.toast_2fa_enabled'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const disableMutation = useMutation({
    mutationFn: (password: string) => api.post('/api/auth/2fa/disable', { password }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
      toast.success(t('security.toast_2fa_disabled'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
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
        <Smartphone aria-hidden="true" className="size-5" />
        <h2 className="font-semibold text-lg">{t('security.two_factor')}</h2>
      </div>

      {status?.enabled && (
        <div className="space-y-4">
          <div className="flex items-center gap-2 text-emerald-600 dark:text-emerald-400">
            <Shield aria-hidden="true" className="size-4" />
            <span className="font-medium text-sm">{t('security.two_factor_enabled')}</span>
          </div>

          {showDisable ? (
            <form className="space-y-3" onSubmit={handleDisable}>
              <p className="text-muted-foreground text-sm">{t('security.enter_password_disable')}</p>
              <Input
                aria-label={t('security.current_password')}
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
              <ShieldOff aria-hidden="true" className="size-4" />
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
          <Button disabled={setupPending} onClick={handleSetup}>
            <Shield aria-hidden="true" className="size-4" />
            {t('security.setup_2fa')}
          </Button>
        </div>
      )}
    </div>
  )
}
