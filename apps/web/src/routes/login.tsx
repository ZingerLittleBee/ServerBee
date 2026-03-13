import { useQuery } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import type React from 'react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { ApiError, api } from '@/lib/api-client'
import type { OAuthProvidersResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/login')({
  component: LoginPage
})

function LoginPage() {
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
    return loginError.message || 'Login failed. Please try again.'
  })()

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <h1 className="font-bold text-2xl">Sign in to ServerBee</h1>
          <p className="mt-1 text-muted-foreground text-sm">Enter your credentials to continue</p>
        </div>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {errorMessage && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm">{errorMessage}</div>
          )}

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="username">
              Username
            </label>
            <input
              autoComplete="username"
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              disabled={needs2FA}
              id="username"
              onChange={(e) => setUsername(e.target.value)}
              placeholder="admin"
              required
              type="text"
              value={username}
            />
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="password">
              Password
            </label>
            <input
              autoComplete="current-password"
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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
                Two-Factor Code
              </label>
              <input
                autoComplete="one-time-code"
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 font-mono text-sm tracking-widest shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                id="totp"
                inputMode="numeric"
                maxLength={6}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                pattern="[0-9]{6}"
                placeholder="000000"
                required
                value={totpCode}
              />
              <p className="text-muted-foreground text-xs">Enter the code from your authenticator app</p>
            </div>
          )}

          <Button className="w-full" disabled={isLoggingIn} type="submit">
            {isLoggingIn ? 'Signing in...' : 'Sign in'}
          </Button>
        </form>

        <OAuthButtons />
      </div>
    </div>
  )
}

const providerConfig: Record<string, { label: string; icon: () => React.JSX.Element }> = {
  github: { label: 'GitHub', icon: GitHubIcon },
  google: { label: 'Google', icon: GoogleIcon }
}

function OAuthButtons() {
  const { data } = useQuery<OAuthProvidersResponse>({
    queryKey: ['auth', 'oauth', 'providers'],
    queryFn: () => api.get<OAuthProvidersResponse>('/api/auth/oauth/providers'),
    retry: false,
    staleTime: 300_000
  })

  const providers = data?.providers ?? []
  if (providers.length === 0) {
    return null
  }

  return (
    <div className="space-y-3">
      <div className="relative">
        <div className="absolute inset-0 flex items-center">
          <span className="w-full border-t" />
        </div>
        <div className="relative flex justify-center text-xs uppercase">
          <span className="bg-background px-2 text-muted-foreground">Or continue with</span>
        </div>
      </div>

      <div className={`grid gap-2 ${providers.length === 1 ? 'grid-cols-1' : 'grid-cols-2'}`}>
        {providers.map((provider) => {
          const config = providerConfig[provider]
          if (!config) {
            return null
          }
          const Icon = config.icon
          return (
            <a
              className="inline-flex h-9 items-center justify-center gap-2 rounded-md border border-input bg-background px-3 text-sm shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground"
              href={`/api/auth/oauth/${provider}`}
              key={provider}
            >
              <Icon />
              {config.label}
            </a>
          )
        })}
      </div>
    </div>
  )
}

function GitHubIcon() {
  return (
    <svg aria-hidden="true" className="size-4" fill="currentColor" viewBox="0 0 24 24">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  )
}

function GoogleIcon() {
  return (
    <svg aria-hidden="true" className="size-4" viewBox="0 0 24 24">
      <path
        d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"
        fill="#4285F4"
      />
      <path
        d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
        fill="#34A853"
      />
      <path
        d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
        fill="#FBBC05"
      />
      <path
        d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
        fill="#EA4335"
      />
    </svg>
  )
}
