# Plan 4: Frontend — TanStack Router + Auth + Dashboard + Server Detail

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete frontend SPA: routing, authentication UI, real-time dashboard with server cards, and server detail with metric charts.

**Architecture:** React 19 SPA with TanStack Router for file-based routing, TanStack Query for server state, WebSocket for real-time updates, Recharts for metric visualization. All data flows through a typed API client.

**Tech Stack:** React 19, TanStack Router, TanStack Query, Recharts, shadcn/ui (base-nova), Tailwind CSS v4, xterm.js

**Depends on:** Plan 1 (REST API), Plan 3 (WebSocket endpoints)

---

## Chunk 1: Dependencies + Router Setup

### Task 1: Install Frontend Dependencies

**Files:**
- Modify: `apps/web/package.json`

- [ ] **Step 1: Install TanStack Router + Query**

Run: `cd apps/web && bun add @tanstack/react-router @tanstack/react-query`

- [ ] **Step 2: Install TanStack Router Vite plugin**

Run: `cd apps/web && bun add -D @tanstack/router-plugin`

- [ ] **Step 3: Install Recharts**

Run: `cd apps/web && bun add recharts`

- [ ] **Step 4: Verify build**

Run: `cd apps/web && bun run build`
Expected: PASS (no route files yet, but no errors)

- [ ] **Step 5: Commit**

```bash
git add apps/web/package.json apps/web/bun.lock
git commit -m "feat(web): install TanStack Router, Query, and Recharts"
```

### Task 2: Configure TanStack Router

**Files:**
- Modify: `apps/web/vite.config.ts`
- Create: `apps/web/src/router.tsx`
- Create: `apps/web/src/routes/__root.tsx`
- Create: `apps/web/src/routes/index.tsx`
- Modify: `apps/web/src/main.tsx`

- [ ] **Step 1: Add TanStack Router plugin to vite.config.ts**

```typescript
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'

export default defineConfig({
  plugins: [
    TanStackRouterVite(),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:9527',
        changeOrigin: true,
        ws: true,
      },
    },
  },
})
```

- [ ] **Step 2: Create router.tsx**

```tsx
import { createRouter } from '@tanstack/react-router'
import { routeTree } from './routeTree.gen'

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
```

- [ ] **Step 3: Create routes/__root.tsx**

```tsx
import { Outlet, createRootRoute } from '@tanstack/react-router'
import { ThemeProvider } from '@/components/theme-provider'

export const Route = createRootRoute({
  component: RootLayout,
})

function RootLayout() {
  return (
    <ThemeProvider>
      <div className="min-h-screen bg-background text-foreground">
        <Outlet />
      </div>
    </ThemeProvider>
  )
}
```

- [ ] **Step 4: Create routes/index.tsx (temporary redirect)**

```tsx
import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/')({
  beforeLoad: () => {
    throw redirect({ to: '/login' })
  },
})
```

- [ ] **Step 5: Update main.tsx to use RouterProvider**

```tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { router } from './router'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
    },
  },
})

const root = document.getElementById('root')
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </StrictMode>,
  )
}
```

- [ ] **Step 6: Delete App.tsx (replaced by router)**

- [ ] **Step 7: Verify dev server starts**

Run: `cd apps/web && bun run dev`
Expected: Vite dev server starts, TanStack Router generates routeTree, page loads at localhost:5173

- [ ] **Step 8: Commit**

```bash
git add apps/web/
git commit -m "feat(web): configure TanStack Router with file-based routing"
```

## Chunk 2: API Client + Auth

### Task 3: API Client

**Files:**
- Create: `apps/web/src/lib/api-client.ts`

- [ ] **Step 1: Write api-client.ts**

```typescript
const BASE_URL = '/api'

interface ApiResponse<T> {
  data: T
}

interface ApiError {
  error: {
    code: string
    message: string
  }
}

class ApiClient {
  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${BASE_URL}${path}`
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    }

    const res = await fetch(url, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
      credentials: 'include',
    })

    if (!res.ok) {
      const err: ApiError = await res.json().catch(() => ({
        error: { code: 'UNKNOWN', message: res.statusText },
      }))
      throw new Error(err.error.message)
    }

    const json: ApiResponse<T> = await res.json()
    return json.data
  }

  get<T>(path: string) {
    return this.request<T>('GET', path)
  }

  post<T>(path: string, body?: unknown) {
    return this.request<T>('POST', path, body)
  }

  put<T>(path: string, body?: unknown) {
    return this.request<T>('PUT', path, body)
  }

  delete<T>(path: string) {
    return this.request<T>('DELETE', path)
  }
}

export const api = new ApiClient()
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/lib/api-client.ts
git commit -m "feat(web): add typed API client with fetch"
```

### Task 4: Auth Hook + Login Page

**Files:**
- Create: `apps/web/src/hooks/use-auth.ts`
- Create: `apps/web/src/routes/login.tsx`

- [ ] **Step 1: Write hooks/use-auth.ts**

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

interface User {
  user_id: string
  username: string
  role: string
}

export function useAuth() {
  const queryClient = useQueryClient()

  const { data: user, isLoading } = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: () => api.get<User>('/auth/me'),
    retry: false,
    staleTime: 60_000,
  })

  const loginMutation = useMutation({
    mutationFn: (credentials: { username: string; password: string }) =>
      api.post<User>('/auth/login', credentials),
    onSuccess: (data) => {
      queryClient.setQueryData(['auth', 'me'], data)
    },
  })

  const logoutMutation = useMutation({
    mutationFn: () => api.post('/auth/logout'),
    onSuccess: () => {
      queryClient.setQueryData(['auth', 'me'], null)
      queryClient.clear()
    },
  })

  return {
    user,
    isLoading,
    isAuthenticated: !!user,
    login: loginMutation.mutateAsync,
    loginError: loginMutation.error,
    isLoggingIn: loginMutation.isPending,
    logout: logoutMutation.mutateAsync,
  }
}
```

- [ ] **Step 2: Write routes/login.tsx**

```tsx
import { useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useAuth } from '@/hooks/use-auth'
import { Button } from '@/components/ui/button'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function LoginPage() {
  const navigate = useNavigate()
  const { login, loginError, isLoggingIn } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await login({ username, password })
      navigate({ to: '/dashboard' })
    } catch {
      // Error handled by loginError
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="w-full max-w-sm space-y-6 p-6">
        <div className="text-center">
          <h1 className="text-2xl font-bold">ServerBee</h1>
          <p className="text-muted-foreground mt-1 text-sm">
            Sign in to your dashboard
          </p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <label htmlFor="username" className="text-sm font-medium">
              Username
            </label>
            <input
              id="username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
              placeholder="admin"
              required
              autoFocus
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="password" className="text-sm font-medium">
              Password
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
              required
            />
          </div>

          {loginError && (
            <p className="text-destructive text-sm">
              {loginError.message}
            </p>
          )}

          <Button type="submit" className="w-full" disabled={isLoggingIn}>
            {isLoggingIn ? 'Signing in...' : 'Sign in'}
          </Button>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Verify it renders**

Run: `cd apps/web && bun run dev`
Navigate to `/login`
Expected: Login form renders correctly

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/hooks/use-auth.ts apps/web/src/routes/login.tsx
git commit -m "feat(web): add auth hook and login page"
```

### Task 5: Authenticated Layout Guard

**Files:**
- Create: `apps/web/src/routes/_authed.tsx`

- [ ] **Step 1: Write routes/_authed.tsx**

```tsx
import { Outlet, createFileRoute, redirect } from '@tanstack/react-router'
import { useAuth } from '@/hooks/use-auth'

export const Route = createFileRoute('/_authed')({
  component: AuthedLayout,
})

function AuthedLayout() {
  const { user, isLoading } = useAuth()

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    )
  }

  if (!user) {
    // Use effect-based redirect instead of throwing in render
    window.location.href = '/login'
    return null
  }

  return (
    <div className="flex min-h-screen">
      {/* Sidebar will be added later */}
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  )
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd apps/web && bun run dev`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed.tsx
git commit -m "feat(web): add authenticated route layout guard"
```

## Chunk 3: Dashboard

### Task 6: WebSocket Client

**Files:**
- Create: `apps/web/src/lib/ws-client.ts`

- [ ] **Step 1: Write ws-client.ts**

```typescript
type MessageHandler = (data: unknown) => void

export class WebSocketClient {
  private url: string
  private ws: WebSocket | null = null
  private reconnectDelay = 1000
  private maxDelay = 30000
  private handlers: MessageHandler[] = []
  private closed = false

  constructor(path: string) {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    this.url = `${protocol}//${window.location.host}${path}`
    this.connect()
  }

  private connect() {
    if (this.closed) return

    this.ws = new WebSocket(this.url)

    this.ws.onopen = () => {
      this.reconnectDelay = 1000
    }

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        for (const handler of this.handlers) {
          handler(data)
        }
      } catch {
        // Binary frame or invalid JSON, ignore
      }
    }

    this.ws.onclose = () => {
      if (!this.closed) {
        this.scheduleReconnect()
      }
    }

    this.ws.onerror = () => {
      this.ws?.close()
    }
  }

  private scheduleReconnect() {
    const jitter = this.reconnectDelay * (0.8 + Math.random() * 0.4)
    setTimeout(() => this.connect(), jitter)
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxDelay)
  }

  onMessage(handler: MessageHandler) {
    this.handlers.push(handler)
  }

  close() {
    this.closed = true
    this.ws?.close()
    this.handlers = []
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/lib/ws-client.ts
git commit -m "feat(web): add WebSocket client with auto-reconnect"
```

### Task 7: Servers WebSocket Hook

**Files:**
- Create: `apps/web/src/hooks/use-servers-ws.ts`

- [ ] **Step 1: Write hooks/use-servers-ws.ts**

```typescript
import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { WebSocketClient } from '@/lib/ws-client'

interface ServerStatus {
  id: string
  name: string
  online: boolean
  last_active: number
  uptime: number
  cpu: number
  mem_used: number
  mem_total: number
  swap_used: number
  swap_total: number
  disk_used: number
  disk_total: number
  net_in_speed: number
  net_out_speed: number
  net_in_transfer: number
  net_out_transfer: number
  load1: number
  load5: number
  load15: number
  tcp_conn: number
  udp_conn: number
  process_count: number
  cpu_name?: string
  os?: string
  region?: string
  country_code?: string
}

interface BrowserMessage {
  type: string
  servers?: ServerStatus[]
  server_id?: string
}

export function useServersWebSocket() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const ws = new WebSocketClient('/api/ws/servers')

    ws.onMessage((raw) => {
      const msg = raw as BrowserMessage

      switch (msg.type) {
        case 'full_sync':
          queryClient.setQueryData<ServerStatus[]>(
            ['servers'],
            msg.servers ?? [],
          )
          break

        case 'update':
          queryClient.setQueryData<ServerStatus[]>(['servers'], (old) => {
            if (!old || !msg.servers) return msg.servers ?? []
            const updated = new Map(old.map((s) => [s.id, s]))
            for (const server of msg.servers) {
              updated.set(server.id, { ...updated.get(server.id), ...server })
            }
            return [...updated.values()]
          })
          break

        case 'server_online':
          queryClient.setQueryData<ServerStatus[]>(['servers'], (old) => {
            if (!old) return old
            return old.map((s) =>
              s.id === msg.server_id ? { ...s, online: true } : s,
            )
          })
          break

        case 'server_offline':
          queryClient.setQueryData<ServerStatus[]>(['servers'], (old) => {
            if (!old) return old
            return old.map((s) =>
              s.id === msg.server_id ? { ...s, online: false } : s,
            )
          })
          break
      }
    })

    return () => ws.close()
  }, [queryClient])
}

export type { ServerStatus }
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): add WebSocket hook for real-time server updates"
```

### Task 8: Dashboard Page

**Files:**
- Create: `apps/web/src/routes/_authed/index.tsx`
- Create: `apps/web/src/components/server/server-card.tsx`
- Create: `apps/web/src/components/server/status-badge.tsx`

- [ ] **Step 1: Create components/server/status-badge.tsx**

```tsx
interface StatusBadgeProps {
  online: boolean
}

export function StatusBadge({ online }: StatusBadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium ${
        online
          ? 'bg-green-500/10 text-green-600 dark:text-green-400'
          : 'bg-red-500/10 text-red-600 dark:text-red-400'
      }`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          online ? 'bg-green-500' : 'bg-red-500'
        }`}
      />
      {online ? 'Online' : 'Offline'}
    </span>
  )
}
```

- [ ] **Step 2: Create components/server/server-card.tsx**

```tsx
import { Link } from '@tanstack/react-router'
import { StatusBadge } from './status-badge'
import type { ServerStatus } from '@/hooks/use-servers-ws'

interface ServerCardProps {
  server: ServerStatus
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / k ** i).toFixed(1)} ${sizes[i]}`
}

function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  if (days > 0) return `${days}d ${hours}h`
  const minutes = Math.floor((seconds % 3600) / 60)
  return `${hours}h ${minutes}m`
}

function ProgressBar({
  value,
  className = '',
}: {
  value: number
  className?: string
}) {
  const color =
    value > 90
      ? 'bg-red-500'
      : value > 70
        ? 'bg-yellow-500'
        : 'bg-green-500'

  return (
    <div className={`bg-muted h-1.5 w-full rounded-full ${className}`}>
      <div
        className={`h-full rounded-full transition-all ${color}`}
        style={{ width: `${Math.min(value, 100)}%` }}
      />
    </div>
  )
}

export function ServerCard({ server }: ServerCardProps) {
  const cpuPercent = server.cpu
  const memPercent =
    server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPercent =
    server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0

  return (
    <Link
      to="/servers/$id"
      params={{ id: server.id }}
      className="bg-card border-border hover:border-primary/20 block rounded-lg border p-4 transition-colors"
    >
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="font-medium">{server.name}</h3>
          {server.region && (
            <span className="text-muted-foreground text-xs">
              {server.region}
            </span>
          )}
        </div>
        <StatusBadge online={server.online} />
      </div>

      {server.online && (
        <div className="space-y-2.5">
          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">CPU</span>
            <span>{cpuPercent.toFixed(1)}%</span>
          </div>
          <ProgressBar value={cpuPercent} />

          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">Memory</span>
            <span>
              {formatBytes(server.mem_used)} / {formatBytes(server.mem_total)}
            </span>
          </div>
          <ProgressBar value={memPercent} />

          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">Disk</span>
            <span>
              {formatBytes(server.disk_used)} / {formatBytes(server.disk_total)}
            </span>
          </div>
          <ProgressBar value={diskPercent} />

          <div className="flex justify-between text-xs">
            <span className="text-muted-foreground">
              {formatSpeed(server.net_in_speed)} ↓
            </span>
            <span className="text-muted-foreground">
              {formatSpeed(server.net_out_speed)} ↑
            </span>
          </div>

          <div className="text-muted-foreground text-right text-xs">
            Uptime: {formatUptime(server.uptime)}
          </div>
        </div>
      )}

      {!server.online && (
        <div className="text-muted-foreground py-4 text-center text-sm">
          Server is offline
        </div>
      )}
    </Link>
  )
}
```

- [ ] **Step 3: Create routes/_authed/index.tsx (Dashboard)**

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import { useServersWebSocket, type ServerStatus } from '@/hooks/use-servers-ws'
import { ServerCard } from '@/components/server/server-card'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage,
})

function DashboardPage() {
  // Initial load via REST
  useQuery({
    queryKey: ['servers'],
    queryFn: () => api.get<ServerStatus[]>('/servers'),
    staleTime: Number.POSITIVE_INFINITY,
  })

  // Real-time updates via WebSocket
  useServersWebSocket()

  const servers = useQuery<ServerStatus[]>({
    queryKey: ['servers'],
    enabled: false,
  }).data

  const online = servers?.filter((s) => s.online).length ?? 0
  const total = servers?.length ?? 0

  return (
    <div className="mx-auto max-w-7xl p-6">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-2xl font-bold">Dashboard</h1>
        <div className="text-muted-foreground text-sm">
          {online} / {total} online
        </div>
      </div>

      {!servers || servers.length === 0 ? (
        <div className="text-muted-foreground py-20 text-center">
          <p>No servers yet.</p>
          <p className="mt-1 text-sm">
            Install the agent on your servers to start monitoring.
          </p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {servers.map((server) => (
            <ServerCard key={server.id} server={server} />
          ))}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Update routes/index.tsx to redirect to dashboard**

```tsx
import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/')({
  beforeLoad: () => {
    throw redirect({ to: '/_authed' })
  },
})
```

- [ ] **Step 5: Verify it renders**

Run: `cd apps/web && bun run dev`
Expected: Dashboard page renders, shows "No servers yet" when no data.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/
git commit -m "feat(web): implement dashboard with real-time server cards"
```

## Chunk 4: Server Detail + Charts

### Task 9: Server Detail Page

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$id.tsx`
- Create: `apps/web/src/components/server/metrics-chart.tsx`
- Create: `apps/web/src/hooks/use-api.ts`

- [ ] **Step 1: Create hooks/use-api.ts with server queries**

```typescript
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

interface ServerDetail {
  id: string
  name: string
  cpu_name?: string
  cpu_cores?: number
  cpu_arch?: string
  os?: string
  kernel_version?: string
  mem_total?: number
  swap_total?: number
  disk_total?: number
  ipv4?: string
  ipv6?: string
  region?: string
  country_code?: string
  agent_version?: string
  group_id?: string
  created_at: string
}

interface Record {
  time: string
  cpu: number
  mem_used: number
  swap_used: number
  disk_used: number
  net_in_speed: number
  net_out_speed: number
  load1: number
  load5: number
  load15: number
  tcp_conn: number
  udp_conn: number
  process_count: number
  temperature?: number
  gpu_usage?: number
}

export function useServer(id: string) {
  return useQuery({
    queryKey: ['servers', id],
    queryFn: () => api.get<ServerDetail>(`/servers/${id}`),
  })
}

export function useServerRecords(
  id: string,
  from: string,
  to: string,
  interval: string = 'auto',
) {
  return useQuery({
    queryKey: ['servers', id, 'records', from, to, interval],
    queryFn: () =>
      api.get<Record[]>(
        `/servers/${id}/records?from=${from}&to=${to}&interval=${interval}`,
      ),
  })
}

export type { ServerDetail, Record }
```

- [ ] **Step 2: Create components/server/metrics-chart.tsx**

```tsx
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import type { Record } from '@/hooks/use-api'

interface MetricsChartProps {
  data: Record[]
  dataKey: string
  title: string
  color?: string
  formatValue?: (value: number) => string
  domain?: [number, number]
}

function formatTime(time: string): string {
  return new Date(time).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  })
}

export function MetricsChart({
  data,
  dataKey,
  title,
  color = '#3b82f6',
  formatValue = (v) => v.toFixed(1),
  domain,
}: MetricsChartProps) {
  return (
    <div className="bg-card border-border rounded-lg border p-4">
      <h3 className="mb-3 text-sm font-medium">{title}</h3>
      <ResponsiveContainer width="100%" height={200}>
        <AreaChart data={data}>
          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
          <XAxis
            dataKey="time"
            tickFormatter={formatTime}
            className="text-muted-foreground"
            fontSize={11}
          />
          <YAxis
            tickFormatter={formatValue}
            className="text-muted-foreground"
            fontSize={11}
            domain={domain}
          />
          <Tooltip
            labelFormatter={(label) => new Date(label).toLocaleString()}
            formatter={(value: number) => [formatValue(value), title]}
          />
          <Area
            type="monotone"
            dataKey={dataKey}
            stroke={color}
            fill={color}
            fillOpacity={0.1}
            strokeWidth={1.5}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}
```

- [ ] **Step 3: Create routes/_authed/servers/$id.tsx**

```tsx
import { useState } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { useServer, useServerRecords } from '@/hooks/use-api'
import { MetricsChart } from '@/components/server/metrics-chart'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { useServersWebSocket, type ServerStatus } from '@/hooks/use-servers-ws'
import { useQuery } from '@tanstack/react-query'

export const Route = createFileRoute('/_authed/servers/$id')({
  component: ServerDetailPage,
})

const TIME_RANGES = [
  { label: '1h', hours: 1 },
  { label: '6h', hours: 6 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 168 },
  { label: '30d', hours: 720 },
] as const

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / k ** i).toFixed(1)} ${sizes[i]}`
}

function ServerDetailPage() {
  const { id } = Route.useParams()
  const [rangeHours, setRangeHours] = useState(1)

  const { data: server } = useServer(id)

  useServersWebSocket()
  const liveServers = useQuery<ServerStatus[]>({
    queryKey: ['servers'],
    enabled: false,
  }).data
  const liveStatus = liveServers?.find((s) => s.id === id)

  const now = new Date()
  const from = new Date(now.getTime() - rangeHours * 3600_000).toISOString()
  const to = now.toISOString()

  const { data: records } = useServerRecords(id, from, to)

  return (
    <div className="mx-auto max-w-7xl p-6">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-bold">
            {server?.name ?? 'Loading...'}
          </h1>
          {liveStatus && <StatusBadge online={liveStatus.online} />}
        </div>

        {server && (
          <div className="text-muted-foreground mt-2 flex flex-wrap gap-4 text-sm">
            {server.os && <span>{server.os}</span>}
            {server.cpu_name && (
              <span>
                {server.cpu_name} ({server.cpu_cores} cores)
              </span>
            )}
            {server.mem_total && <span>RAM: {formatBytes(server.mem_total)}</span>}
            {server.ipv4 && <span>{server.ipv4}</span>}
            {server.region && <span>{server.region}</span>}
            {server.agent_version && <span>v{server.agent_version}</span>}
          </div>
        )}
      </div>

      {/* Time Range Selector */}
      <div className="mb-4 flex gap-1">
        {TIME_RANGES.map((range) => (
          <Button
            key={range.label}
            variant={rangeHours === range.hours ? 'default' : 'outline'}
            size="sm"
            onClick={() => setRangeHours(range.hours)}
          >
            {range.label}
          </Button>
        ))}
      </div>

      {/* Charts */}
      {records && records.length > 0 ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <MetricsChart
            data={records}
            dataKey="cpu"
            title="CPU Usage (%)"
            color="#3b82f6"
            domain={[0, 100]}
            formatValue={(v) => `${v.toFixed(1)}%`}
          />
          <MetricsChart
            data={records}
            dataKey="mem_used"
            title="Memory Usage"
            color="#8b5cf6"
            formatValue={(v) => formatBytes(v)}
          />
          <MetricsChart
            data={records}
            dataKey="disk_used"
            title="Disk Usage"
            color="#f59e0b"
            formatValue={(v) => formatBytes(v)}
          />
          <MetricsChart
            data={records}
            dataKey="net_in_speed"
            title="Network In"
            color="#10b981"
            formatValue={(v) => `${formatBytes(v)}/s`}
          />
          <MetricsChart
            data={records}
            dataKey="net_out_speed"
            title="Network Out"
            color="#ef4444"
            formatValue={(v) => `${formatBytes(v)}/s`}
          />
          <MetricsChart
            data={records}
            dataKey="load1"
            title="Load Average (1m)"
            color="#06b6d4"
            formatValue={(v) => v.toFixed(2)}
          />
        </div>
      ) : (
        <div className="text-muted-foreground py-20 text-center">
          No metric data available for this time range.
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Verify charts render**

Run: `cd apps/web && bun run dev`
Expected: Server detail page renders with chart placeholders (data comes from API)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/
git commit -m "feat(web): implement server detail page with metric charts"
```

## Chunk 5: Layout + Navigation

### Task 10: Sidebar + Header Layout

**Files:**
- Create: `apps/web/src/components/layout/sidebar.tsx`
- Create: `apps/web/src/components/layout/header.tsx`
- Create: `apps/web/src/components/layout/theme-toggle.tsx`
- Modify: `apps/web/src/routes/_authed.tsx`

- [ ] **Step 1: Create components/layout/theme-toggle.tsx**

```tsx
import { useTheme } from '@/components/theme-provider'
import { Button } from '@/components/ui/button'

export function ThemeToggle() {
  const { theme, setTheme } = useTheme()

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
      aria-label="Toggle theme"
    >
      {theme === 'dark' ? '☀' : '☾'}
    </Button>
  )
}
```

- [ ] **Step 2: Create components/layout/sidebar.tsx**

Navigation links: Dashboard, Servers, Alerts (P1), Notifications (P1), Ping Tasks (P1), Settings.
Active link highlighting based on current route.

- [ ] **Step 3: Create components/layout/header.tsx**

Top bar with: ServerBee logo, breadcrumb, theme toggle, user menu (username + logout).

- [ ] **Step 4: Update _authed.tsx to include sidebar + header**

```tsx
function AuthedLayout() {
  const { user, isLoading, logout } = useAuth()

  if (isLoading) {
    return <div className="flex min-h-screen items-center justify-center">Loading...</div>
  }

  if (!user) {
    window.location.href = '/login'
    return null
  }

  return (
    <div className="flex min-h-screen">
      <Sidebar />
      <div className="flex flex-1 flex-col">
        <Header user={user} onLogout={logout} />
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
```

- [ ] **Step 5: Verify layout renders**

Run: `cd apps/web && bun run dev`
Expected: Sidebar + header + content area all render correctly

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/
git commit -m "feat(web): add sidebar navigation and header layout"
```

### Task 11: Settings + API Key Management Pages

**Files:**
- Create: `apps/web/src/routes/_authed/settings/index.tsx`
- Create: `apps/web/src/routes/_authed/settings/api-keys.tsx`

- [ ] **Step 1: Create routes/_authed/settings/index.tsx**

Basic settings page showing auto-discovery key and system config options.

- [ ] **Step 2: Create routes/_authed/settings/api-keys.tsx**

API key management page: list existing keys (showing prefix + name + last used), create new key (shows plaintext once), delete key.

- [ ] **Step 3: Verify pages render**

Run: `cd apps/web && bun run dev`
Expected: Settings and API keys pages render with forms

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/
git commit -m "feat(web): add settings and API key management pages"
```

### Task 12: Build Verification

**Files:** None — verification only

- [ ] **Step 1: Full production build**

Run: `cd apps/web && bun run build`
Expected: Build succeeds with no TypeScript errors, outputs to `dist/`

- [ ] **Step 2: Check bundle size**

Run: `ls -lh apps/web/dist/assets/`
Expected: Main JS bundle < 500KB gzipped

- [ ] **Step 3: Verify with server (if Plan 1-3 complete)**

Run server with `RUST_EMBED` pointed at `apps/web/dist/`, verify SPA loads correctly at `http://localhost:9527/`.

- [ ] **Step 4: Final commit**

```bash
git add apps/web/
git commit -m "feat(web): complete P0 frontend with dashboard, server detail, auth, and settings"
```
