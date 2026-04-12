import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
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
  const [deleteKeyId, setDeleteKeyId] = useState<string | null>(null)

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
      toast.success(t('api_keys.toast_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
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
      toast.success(t('api_keys.toast_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
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
              aria-label={t('api_keys.key_name')}
              autoComplete="off"
              className="flex-1"
              name="key-name"
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
                        {apiKey.key_prefix}
                        {'\u2026'}
                      </span>
                      <span>
                        {t('api_keys.created')} {new Date(apiKey.created_at).toLocaleDateString()}
                      </span>
                    </div>
                  </div>
                  <AlertDialog
                    onOpenChange={(open) => {
                      if (!open) {
                        setDeleteKeyId(null)
                      }
                    }}
                    open={deleteKeyId === apiKey.id}
                  >
                    <AlertDialogTrigger
                      onClick={() => setDeleteKeyId(apiKey.id)}
                      render={
                        <Button
                          aria-label={`${t('common:delete')} ${apiKey.name}`}
                          disabled={deleteMutation.isPending}
                          size="sm"
                          variant="destructive"
                        />
                      }
                    >
                      <Trash2 aria-hidden="true" className="size-3.5" />
                    </AlertDialogTrigger>
                    <AlertDialogContent>
                      <AlertDialogHeader>
                        <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                        <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
                      </AlertDialogHeader>
                      <AlertDialogFooter>
                        <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                        <AlertDialogAction
                          onClick={() => {
                            handleDelete(apiKey.id)
                            setDeleteKeyId(null)
                          }}
                          variant="destructive"
                        >
                          {t('common:delete')}
                        </AlertDialogAction>
                      </AlertDialogFooter>
                    </AlertDialogContent>
                  </AlertDialog>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
