import { type FormEvent, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { api } from '@/lib/api-client'

interface PasswordFormState {
  changeError: Error | null
  changePending: boolean
  confirmPassword: string
  newPassword: string
  oldPassword: string
}

type PasswordFormAction =
  | { type: 'changeFailed'; error: Error }
  | { type: 'changeStarted' }
  | { type: 'changeSucceeded' }
  | { type: 'setConfirmPassword'; value: string }
  | { type: 'setNewPassword'; value: string }
  | { type: 'setOldPassword'; value: string }

const INITIAL_PASSWORD_FORM: PasswordFormState = {
  changeError: null,
  changePending: false,
  confirmPassword: '',
  newPassword: '',
  oldPassword: ''
}

function passwordFormReducer(state: PasswordFormState, action: PasswordFormAction): PasswordFormState {
  switch (action.type) {
    case 'changeFailed':
      return { ...state, changeError: action.error, changePending: false }
    case 'changeStarted':
      return { ...state, changeError: null, changePending: true }
    case 'changeSucceeded':
      return INITIAL_PASSWORD_FORM
    case 'setConfirmPassword':
      return { ...state, confirmPassword: action.value }
    case 'setNewPassword':
      return { ...state, newPassword: action.value }
    case 'setOldPassword':
      return { ...state, oldPassword: action.value }
    default:
      return state
  }
}

export function ChangePasswordSection() {
  const { t } = useTranslation(['settings', 'common'])
  const [state, dispatch] = useReducer(passwordFormReducer, INITIAL_PASSWORD_FORM)

  const handlePasswordChange = async (payload: { new_password: string; old_password: string }) => {
    dispatch({ type: 'changeStarted' })
    try {
      await api.put('/api/auth/password', payload)
      dispatch({ type: 'changeSucceeded' })
      toast.success(t('security.toast_password_changed'))
    } catch (err) {
      const error = err instanceof Error ? err : new Error(t('common:errors.operation_failed'))
      dispatch({ type: 'changeFailed', error })
      toast.error(error.message)
    }
  }

  const passwordsMatch = state.newPassword === state.confirmPassword

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!passwordsMatch) {
      toast.error(t('security.password_mismatch'))
      return
    }
    handlePasswordChange({ old_password: state.oldPassword, new_password: state.newPassword }).catch(() => undefined)
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
            onChange={(e) => dispatch({ type: 'setOldPassword', value: e.target.value })}
            required
            type="password"
            value={state.oldPassword}
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
            onChange={(e) => dispatch({ type: 'setNewPassword', value: e.target.value })}
            required
            type="password"
            value={state.newPassword}
          />
        </div>
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="confirm-pw">
            {t('security.confirm_password')}
          </label>
          <Input
            autoComplete="new-password"
            id="confirm-pw"
            minLength={8}
            onChange={(e) => dispatch({ type: 'setConfirmPassword', value: e.target.value })}
            required
            type="password"
            value={state.confirmPassword}
          />
          {state.confirmPassword.length > 0 && !passwordsMatch && (
            <p className="text-destructive text-sm">{t('security.password_mismatch')}</p>
          )}
        </div>

        {state.changeError && (
          <p className="text-destructive text-sm">{state.changeError.message || t('security.change_failed')}</p>
        )}

        <Button disabled={state.changePending || !passwordsMatch || state.newPassword.length === 0} type="submit">
          {state.changePending ? t('security.changing') : t('security.change_password')}
        </Button>
      </form>
    </div>
  )
}
