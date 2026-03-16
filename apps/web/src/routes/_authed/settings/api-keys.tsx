import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { ApiKeyResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/api-keys')({
  component: ApiKeysPage
})

function ApiKeysPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [newKeyName, setNewKeyName] = useState('')
  const [createdKey, setCreatedKey] = useState<string | null>(null)

  const { data: keys, isLoading } = useQuery<ApiKeyResponse[]>({
    queryKey: ['settings', 'api-keys'],
    queryFn: () => api.get<ApiKeyResponse[]>('/api/auth/api-keys')
  })

  const createMutation = useMutation({
    mutationFn: (name: string) => api.post<ApiKeyResponse>('/api/auth/api-keys', { name }),
    onSuccess: (data) => {
      setCreatedKey(data.key ?? null)
      setNewKeyName('')
      queryClient
        .invalidateQueries({
          queryKey: ['settings', 'api-keys']
        })
        .catch(() => {
          // Invalidation error is non-critical
        })
      toast.success('API key created')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
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
      toast.success('API key deleted')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
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
      <h1 className="mb-6 font-bold text-2xl">{t('api_keys.title')}</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-4 font-semibold text-lg">{t('api_keys.create_new')}</h2>

          {createdKey && (
            <div className="mb-4 rounded-md border border-emerald-500/30 bg-emerald-500/10 p-3">
              <p className="mb-1 font-medium text-sm">{t('api_keys.new_key_warning')}</p>
              <code className="block break-all rounded bg-muted px-2 py-1 font-mono text-sm">{createdKey}</code>
            </div>
          )}

          <form className="flex gap-2" onSubmit={handleCreate}>
            <Input
              className="flex-1"
              onChange={(e) => setNewKeyName(e.target.value)}
              placeholder={t('api_keys.key_name')}
              required
              type="text"
              value={newKeyName}
            />
            <Button disabled={createMutation.isPending} type="submit">
              <Plus className="size-4" />
              {t('common:create')}
            </Button>
          </form>
        </div>

        <div className="rounded-lg border bg-card">
          <div className="border-b px-6 py-4">
            <h2 className="font-semibold text-lg">{t('api_keys.active_keys')}</h2>
          </div>

          {isLoading && (
            <div className="space-y-3 p-6">
              {Array.from({ length: 3 }, (_, i) => (
                <Skeleton className="h-12" key={`skeleton-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!keys || keys.length === 0) && (
            <div className="p-6 text-center text-muted-foreground text-sm">{t('api_keys.no_keys')}</div>
          )}
          {!isLoading && keys && keys.length > 0 && (
            <div className="divide-y">
              {keys.map((apiKey) => (
                <div className="flex items-center justify-between px-6 py-3" key={apiKey.id}>
                  <div>
                    <p className="font-medium text-sm">{apiKey.name}</p>
                    <div className="flex gap-3 text-muted-foreground text-xs">
                      <span className="font-mono">
                        {t('api_keys.prefix')}
                        {apiKey.key_prefix}...
                      </span>
                      <span>
                        {t('api_keys.created')} {new Date(apiKey.created_at).toLocaleDateString()}
                      </span>
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
