import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/api-keys')({
  component: ApiKeysPage
})

interface ApiKey {
  created_at: string
  id: string
  key: string | null
  key_prefix: string
  name: string
}

function ApiKeysPage() {
  const queryClient = useQueryClient()
  const [newKeyName, setNewKeyName] = useState('')
  const [createdKey, setCreatedKey] = useState<string | null>(null)

  const { data: keys, isLoading } = useQuery<ApiKey[]>({
    queryKey: ['settings', 'api-keys'],
    queryFn: () => api.get<ApiKey[]>('/api/auth/api-keys')
  })

  const createMutation = useMutation({
    mutationFn: (name: string) => api.post<ApiKey>('/api/auth/api-keys', { name }),
    onSuccess: (data) => {
      setCreatedKey(data.key)
      setNewKeyName('')
      queryClient
        .invalidateQueries({
          queryKey: ['settings', 'api-keys']
        })
        .catch(() => {
          // Invalidation error is non-critical
        })
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/auth/api-keys/${id}`),
    onSuccess: () => {
      queryClient
        .invalidateQueries({
          queryKey: ['settings', 'api-keys']
        })
        .catch(() => {
          // Invalidation error is non-critical
        })
    }
  })

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (newKeyName.trim().length === 0) {
      return
    }
    createMutation.mutate(newKeyName.trim())
  }

  const handleDelete = (id: string) => {
    deleteMutation.mutate(id)
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">API Keys</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-4 font-semibold text-lg">Create New Key</h2>

          {createdKey && (
            <div className="mb-4 rounded-md border border-emerald-500/30 bg-emerald-500/10 p-3">
              <p className="mb-1 font-medium text-sm">Your new API key (copy it now, it will not be shown again):</p>
              <code className="block break-all rounded bg-muted px-2 py-1 font-mono text-sm">{createdKey}</code>
            </div>
          )}

          <form className="flex gap-2" onSubmit={handleCreate}>
            <input
              className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              onChange={(e) => setNewKeyName(e.target.value)}
              placeholder="Key name (e.g. CI/CD)"
              required
              type="text"
              value={newKeyName}
            />
            <Button disabled={createMutation.isPending} type="submit">
              <Plus className="size-4" />
              Create
            </Button>
          </form>
        </div>

        <div className="rounded-lg border bg-card">
          <div className="border-b px-6 py-4">
            <h2 className="font-semibold text-lg">Active Keys</h2>
          </div>

          {isLoading && (
            <div className="space-y-3 p-6">
              {Array.from({ length: 3 }, (_, i) => (
                <div className="h-12 animate-pulse rounded bg-muted" key={`skeleton-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!keys || keys.length === 0) && (
            <div className="p-6 text-center text-muted-foreground text-sm">No API keys created yet</div>
          )}
          {!isLoading && keys && keys.length > 0 && (
            <div className="divide-y">
              {keys.map((apiKey) => (
                <div className="flex items-center justify-between px-6 py-3" key={apiKey.id}>
                  <div>
                    <p className="font-medium text-sm">{apiKey.name}</p>
                    <div className="flex gap-3 text-muted-foreground text-xs">
                      <span className="font-mono">sb_{apiKey.key_prefix}...</span>
                      <span>Created: {new Date(apiKey.created_at).toLocaleDateString()}</span>
                    </div>
                  </div>
                  <Button
                    aria-label={`Delete key ${apiKey.name}`}
                    disabled={deleteMutation.isPending}
                    onClick={() => handleDelete(apiKey.id)}
                    size="sm"
                    variant="destructive"
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
