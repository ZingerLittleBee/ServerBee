import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Loader2, Shield, ShieldOff, Smartphone } from 'lucide-react'
import { type FormEvent, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { TotpSetupResponse, TotpStatusResponse } from '@/lib/api-schema'

interface TwoFactorState {
  disablePassword: string
  setupData: TotpSetupResponse | null
  setupPending: boolean
  showDisable: boolean
  verifyCode: string
}

type TwoFactorAction =
  | { type: 'cancelDisable' }
  | { type: 'cancelSetup' }
  | { type: 'disableSubmitted' }
  | { type: 'enableSucceeded' }
  | { type: 'setDisablePassword'; value: string }
  | { type: 'setShowDisable'; value: boolean }
  | { type: 'setVerifyCode'; value: string }
  | { type: 'setupFailed' }
  | { type: 'setupStarted' }
  | { type: 'setupSucceeded'; data: TotpSetupResponse }

const INITIAL_TWO_FACTOR_STATE: TwoFactorState = {
  disablePassword: '',
  setupData: null,
  setupPending: false,
  showDisable: false,
  verifyCode: ''
}

function twoFactorReducer(state: TwoFactorState, action: TwoFactorAction): TwoFactorState {
  switch (action.type) {
    case 'cancelDisable':
      return { ...state, disablePassword: '', showDisable: false }
    case 'cancelSetup':
    case 'enableSucceeded':
      return { ...state, setupData: null, verifyCode: '' }
    case 'disableSubmitted':
      return { ...state, disablePassword: '', showDisable: false }
    case 'setDisablePassword':
      return { ...state, disablePassword: action.value }
    case 'setShowDisable':
      return { ...state, showDisable: action.value }
    case 'setVerifyCode':
      return { ...state, verifyCode: action.value }
    case 'setupFailed':
      return { ...state, setupPending: false }
    case 'setupStarted':
      return { ...state, setupPending: true }
    case 'setupSucceeded':
      return { ...state, setupData: action.data, setupPending: false }
    default:
      return state
  }
}

export function TwoFactorSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [state, dispatch] = useReducer(twoFactorReducer, INITIAL_TWO_FACTOR_STATE)

  const { data: status, isLoading } = useQuery<TotpStatusResponse>({
    queryKey: ['auth', '2fa', 'status'],
    queryFn: () => api.get<TotpStatusResponse>('/api/auth/2fa/status')
  })

  const handleSetup = async () => {
    if (state.setupPending) {
      return
    }
    dispatch({ type: 'setupStarted' })
    try {
      const data = await api.post<TotpSetupResponse>('/api/auth/2fa/setup')
      dispatch({ type: 'setupSucceeded', data })
      toast.success(t('security.toast_2fa_setup'))
    } catch (err) {
      dispatch({ type: 'setupFailed' })
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  }

  const enableMutation = useMutation({
    mutationFn: (code: string) => api.post('/api/auth/2fa/enable', { code }),
    onSuccess: () => {
      dispatch({ type: 'enableSucceeded' })
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
    if (!state.setupData || state.verifyCode.length !== 6) {
      return
    }
    enableMutation.mutate(state.verifyCode)
  }

  const handleDisable = (e: FormEvent) => {
    e.preventDefault()
    if (state.disablePassword.length === 0) {
      return
    }
    disableMutation.mutate(state.disablePassword)
    dispatch({ type: 'disableSubmitted' })
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

          {state.showDisable ? (
            <form className="space-y-3" onSubmit={handleDisable}>
              <p className="text-muted-foreground text-sm">{t('security.enter_password_disable')}</p>
              <Input
                aria-label={t('security.current_password')}
                autoComplete="current-password"
                className="max-w-xs"
                onChange={(e) => dispatch({ type: 'setDisablePassword', value: e.target.value })}
                placeholder={t('security.current_password')}
                required
                type="password"
                value={state.disablePassword}
              />
              <div className="flex gap-2">
                <Button disabled={disableMutation.isPending} type="submit" variant="destructive">
                  {t('security.confirm_disable')}
                </Button>
                <Button onClick={() => dispatch({ type: 'cancelDisable' })} type="button" variant="outline">
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
            <Button onClick={() => dispatch({ type: 'setShowDisable', value: true })} variant="destructive">
              <ShieldOff aria-hidden="true" className="size-4" />
              {t('security.disable_2fa')}
            </Button>
          )}
        </div>
      )}
      {!status?.enabled && state.setupData && (
        <div className="space-y-4">
          <p className="text-muted-foreground text-sm">{t('security.scan_qr')}</p>

          <div className="flex justify-center rounded-md border bg-white p-4">
            <img
              alt={t('security.qr_alt')}
              height={192}
              src={`data:image/png;base64,${state.setupData.qr_code_base64}`}
              width={192}
            />
          </div>

          <details className="text-sm">
            <summary className="cursor-pointer text-muted-foreground">{t('security.cant_scan')}</summary>
            <code className="mt-1 block break-all rounded bg-muted px-2 py-1 font-mono text-xs">
              {state.setupData.secret}
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
              onChange={(e) => dispatch({ type: 'setVerifyCode', value: e.target.value.replace(/\D/g, '') })}
              pattern="[0-9]{6}"
              placeholder="000000"
              required
              value={state.verifyCode}
            />
            <div className="flex gap-2">
              <Button disabled={enableMutation.isPending || state.verifyCode.length !== 6} type="submit">
                {enableMutation.isPending ? <Loader2 className="size-4 animate-spin" /> : <Check className="size-4" />}
                {t('security.verify_enable')}
              </Button>
              <Button onClick={() => dispatch({ type: 'cancelSetup' })} type="button" variant="outline">
                {t('common:cancel')}
              </Button>
            </div>
            {enableMutation.error && <p className="text-destructive text-sm">{t('security.invalid_code')}</p>}
          </form>
        </div>
      )}
      {!(status?.enabled || state.setupData) && (
        <div className="space-y-3">
          <p className="text-muted-foreground text-sm">{t('security.two_factor_description')}</p>
          <Button disabled={state.setupPending} onClick={handleSetup}>
            <Shield aria-hidden="true" className="size-4" />
            {t('security.setup_2fa')}
          </Button>
        </div>
      )}
    </div>
  )
}
