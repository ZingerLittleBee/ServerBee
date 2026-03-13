import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Check, Link2Off, Loader2, Shield, ShieldOff, Smartphone } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { OAuthAccount, TotpSetupResponse, TotpStatusResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/security')({
  component: SecurityPage
})

function SecurityPage() {
  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Security</h1>
      <div className="max-w-2xl space-y-8">
        <TwoFactorSection />
        <ChangePasswordSection />
        <OAuthAccountsSection />
      </div>
    </div>
  )
}

function TwoFactorSection() {
  const queryClient = useQueryClient()
  const [setupData, setSetupData] = useState<TotpSetupResponse | null>(null)
  const [verifyCode, setVerifyCode] = useState('')

  const { data: status, isLoading } = useQuery<TotpStatusResponse>({
    queryKey: ['auth', '2fa', 'status'],
    queryFn: () => api.get<TotpStatusResponse>('/api/auth/2fa/status')
  })

  const setupMutation = useMutation({
    mutationFn: () => api.post<TotpSetupResponse>('/api/auth/2fa/setup'),
    onSuccess: (data) => setSetupData(data)
  })

  const enableMutation = useMutation({
    mutationFn: (code: string) => api.post('/api/auth/2fa/enable', { code }),
    onSuccess: () => {
      setSetupData(null)
      setVerifyCode('')
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
    }
  })

  const disableMutation = useMutation({
    mutationFn: (password: string) => api.post('/api/auth/2fa/disable', { password }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', '2fa', 'status'] }).catch(() => undefined)
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
        <div className="h-20 animate-pulse rounded bg-muted" />
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-6">
      <div className="mb-4 flex items-center gap-2">
        <Smartphone className="size-5" />
        <h2 className="font-semibold text-lg">Two-Factor Authentication</h2>
      </div>

      {status?.enabled && (
        <div className="space-y-4">
          <div className="flex items-center gap-2 text-emerald-600 dark:text-emerald-400">
            <Shield className="size-4" />
            <span className="font-medium text-sm">2FA is enabled</span>
          </div>

          {showDisable ? (
            <form className="space-y-3" onSubmit={handleDisable}>
              <p className="text-muted-foreground text-sm">Enter your password to disable 2FA:</p>
              <input
                autoComplete="current-password"
                className="flex h-9 w-full max-w-xs rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setDisablePassword(e.target.value)}
                placeholder="Current password"
                required
                type="password"
                value={disablePassword}
              />
              <div className="flex gap-2">
                <Button disabled={disableMutation.isPending} type="submit" variant="destructive">
                  Confirm Disable
                </Button>
                <Button
                  onClick={() => {
                    setShowDisable(false)
                    setDisablePassword('')
                  }}
                  type="button"
                  variant="outline"
                >
                  Cancel
                </Button>
              </div>
              {disableMutation.error && (
                <p className="text-destructive text-sm">{disableMutation.error.message || 'Failed to disable 2FA'}</p>
              )}
            </form>
          ) : (
            <Button onClick={() => setShowDisable(true)} variant="destructive">
              <ShieldOff className="size-4" />
              Disable 2FA
            </Button>
          )}
        </div>
      )}
      {!status?.enabled && setupData && (
        <div className="space-y-4">
          <p className="text-muted-foreground text-sm">
            Scan the QR code with your authenticator app (Google Authenticator, Authy, etc.)
          </p>

          <div className="flex justify-center rounded-md border bg-white p-4">
            <img
              alt="TOTP QR Code"
              height={192}
              src={`data:image/png;base64,${setupData.qr_code_base64}`}
              width={192}
            />
          </div>

          <details className="text-sm">
            <summary className="cursor-pointer text-muted-foreground">Can&apos;t scan? Enter this key manually</summary>
            <code className="mt-1 block break-all rounded bg-muted px-2 py-1 font-mono text-xs">
              {setupData.secret}
            </code>
          </details>

          <form className="space-y-3" onSubmit={handleEnable}>
            <label className="font-medium text-sm" htmlFor="totp-code">
              Enter the 6-digit code from your authenticator
            </label>
            <input
              autoComplete="one-time-code"
              className="flex h-9 w-full max-w-xs rounded-md border border-input bg-transparent px-3 py-1 font-mono text-sm tracking-widest shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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
                Verify & Enable
              </Button>
              <Button
                onClick={() => {
                  setSetupData(null)
                  setVerifyCode('')
                }}
                type="button"
                variant="outline"
              >
                Cancel
              </Button>
            </div>
            {enableMutation.error && <p className="text-destructive text-sm">Invalid code. Please try again.</p>}
          </form>
        </div>
      )}
      {!(status?.enabled || setupData) && (
        <div className="space-y-3">
          <p className="text-muted-foreground text-sm">
            Add an extra layer of security to your account using a time-based one-time password (TOTP).
          </p>
          <Button disabled={setupMutation.isPending} onClick={() => setupMutation.mutate()}>
            <Shield className="size-4" />
            Set Up 2FA
          </Button>
        </div>
      )}
    </div>
  )
}

function ChangePasswordSection() {
  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [success, setSuccess] = useState(false)

  const mutation = useMutation({
    mutationFn: (payload: { new_password: string; old_password: string }) => api.put('/api/auth/password', payload),
    onSuccess: () => {
      setOldPassword('')
      setNewPassword('')
      setSuccess(true)
      setTimeout(() => setSuccess(false), 3000)
    }
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    mutation.mutate({ old_password: oldPassword, new_password: newPassword })
  }

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-4 font-semibold text-lg">Change Password</h2>

      <form className="max-w-xs space-y-3" onSubmit={handleSubmit}>
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="old-pw">
            Current password
          </label>
          <input
            autoComplete="current-password"
            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            id="old-pw"
            onChange={(e) => setOldPassword(e.target.value)}
            required
            type="password"
            value={oldPassword}
          />
        </div>
        <div className="space-y-1">
          <label className="font-medium text-sm" htmlFor="new-pw">
            New password
          </label>
          <input
            autoComplete="new-password"
            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            id="new-pw"
            minLength={8}
            onChange={(e) => setNewPassword(e.target.value)}
            required
            type="password"
            value={newPassword}
          />
        </div>

        {mutation.error && (
          <p className="text-destructive text-sm">{mutation.error.message || 'Failed to change password'}</p>
        )}
        {success && <p className="text-emerald-600 text-sm dark:text-emerald-400">Password changed successfully</p>}

        <Button disabled={mutation.isPending} type="submit">
          {mutation.isPending ? 'Changing...' : 'Change Password'}
        </Button>
      </form>
    </div>
  )
}

function OAuthAccountsSection() {
  const queryClient = useQueryClient()

  const { data: accounts, isLoading } = useQuery<OAuthAccount[]>({
    queryKey: ['auth', 'oauth', 'accounts'],
    queryFn: () => api.get<OAuthAccount[]>('/api/auth/oauth/accounts')
  })

  const unlinkMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/auth/oauth/accounts/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', 'oauth', 'accounts'] }).catch(() => undefined)
    }
  })

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-4 font-semibold text-lg">Linked Accounts</h2>

      {isLoading && (
        <div className="space-y-2">
          <div className="h-12 animate-pulse rounded bg-muted" />
          <div className="h-12 animate-pulse rounded bg-muted" />
        </div>
      )}
      {!isLoading && (!accounts || accounts.length === 0) && (
        <p className="text-muted-foreground text-sm">No linked OAuth accounts</p>
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
                aria-label={`Unlink ${acct.provider} account`}
                disabled={unlinkMutation.isPending}
                onClick={() => unlinkMutation.mutate(acct.id)}
                size="sm"
                variant="outline"
              >
                <Link2Off className="size-3.5" />
                Unlink
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
