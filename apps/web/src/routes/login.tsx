import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'

export const Route = createFileRoute('/login')({
  component: LoginPage
})

function LoginPage() {
  const navigate = useNavigate()
  const { login, loginError, isLoggingIn } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    try {
      await login({ username, password })
      await navigate({ to: '/' })
    } catch {
      // Error is captured in loginError
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <h1 className="font-bold text-2xl">Sign in to ServerBee</h1>
          <p className="mt-1 text-muted-foreground text-sm">Enter your credentials to continue</p>
        </div>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {loginError && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm">
              {loginError.message || 'Login failed. Please try again.'}
            </div>
          )}

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="username">
              Username
            </label>
            <input
              autoComplete="username"
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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
              id="password"
              onChange={(e) => setPassword(e.target.value)}
              required
              type="password"
              value={password}
            />
          </div>

          <Button className="w-full" disabled={isLoggingIn} type="submit">
            {isLoggingIn ? 'Signing in...' : 'Sign in'}
          </Button>
        </form>
      </div>
    </div>
  )
}
