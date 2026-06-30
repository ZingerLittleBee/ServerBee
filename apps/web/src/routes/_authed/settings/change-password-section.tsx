import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { api } from '@/lib/api-client'

export function ChangePasswordSection() {
  const { t } = useTranslation(['settings', 'common'])
  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [changePending, setChangePending] = useState(false)
  const [changeError, setChangeError] = useState<Error | null>(null)

  const handlePasswordChange = async (payload: { new_password: string; old_password: string }) => {
    setChangePending(true)
    setChangeError(null)
    try {
      await api.put('/api/auth/password', payload)
      setOldPassword('')
      setNewPassword('')
      setConfirmPassword('')
      toast.success(t('security.toast_password_changed'))
    } catch (err) {
      const error = err instanceof Error ? err : new Error(t('common:errors.operation_failed'))
      setChangeError(error)
      toast.error(error.message)
    } finally {
      setChangePending(false)
    }
  }

  const passwordsMatch = newPassword === confirmPassword

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!passwordsMatch) {
      toast.error(t('security.password_mismatch'))
      return
    }
    handlePasswordChange({ old_password: oldPassword, new_password: newPassword }).catch(() => undefined)
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
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="confirm-pw">
            {t('security.confirm_password')}
          </label>
          <Input
            autoComplete="new-password"
            id="confirm-pw"
            minLength={8}
            onChange={(e) => setConfirmPassword(e.target.value)}
            required
            type="password"
            value={confirmPassword}
          />
          {confirmPassword.length > 0 && !passwordsMatch && (
            <p className="text-destructive text-sm">{t('security.password_mismatch')}</p>
          )}
        </div>

        {changeError && (
          <p className="text-destructive text-sm">{changeError.message || t('security.change_failed')}</p>
        )}

        <Button disabled={changePending || !passwordsMatch || newPassword.length === 0} type="submit">
          {changePending ? t('security.changing') : t('security.change_password')}
        </Button>
      </form>
    </div>
  )
}
