import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Pencil, Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
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
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'
import type {
  CreateMaintenanceRequest,
  MaintenanceItem,
  ServerResponse,
  UpdateMaintenanceRequest
} from '@/lib/api-schema'
import { StatusPageMaintenanceFormDialog } from './status-page-maintenance-form-dialog'

export function StatusPageMaintenanceTab({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<MaintenanceItem | null>(null)
  const [deleteId, setDeleteId] = useState<string | null>(null)

  const { data: maintenances, isLoading } = useQuery<MaintenanceItem[]>({
    queryKey: ['maintenances'],
    queryFn: () => api.get<MaintenanceItem[]>('/api/maintenances')
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateMaintenanceRequest) => api.post('/api/maintenances', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['maintenances'] }).catch(() => undefined)
      setDialogOpen(false)
      toast.success(t('maintenance.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateMaintenanceRequest }) =>
      api.put(`/api/maintenances/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['maintenances'] }).catch(() => undefined)
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('maintenance.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/maintenances/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['maintenances'] }).catch(() => undefined)
      toast.success(t('maintenance.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (data: CreateMaintenanceRequest | UpdateMaintenanceRequest, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data as UpdateMaintenanceRequest })
    } else {
      createMutation.mutate(data as CreateMaintenanceRequest)
    }
  }

  return (
    <div>
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-muted-foreground text-sm">{t('maintenance.tab_description')}</p>
        <Button
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="size-4" />
          {t('maintenance.create')}
        </Button>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!isLoading && (!maintenances || maintenances.length === 0) && (
        <div className="rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">{t('maintenance.empty')}</p>
        </div>
      )}

      {maintenances && maintenances.length > 0 && (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('maintenance.field_title')}</TableHead>
                <TableHead>{t('maintenance.field_start')}</TableHead>
                <TableHead>{t('maintenance.field_end')}</TableHead>
                <TableHead>{t('maintenance.field_active')}</TableHead>
                <TableHead>{t('maintenance.field_is_public')}</TableHead>
                <TableHead className="text-right">{t('status_pages.col_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {maintenances.map((maintenance) => (
                <TableRow key={maintenance.id}>
                  <TableCell className="font-medium">{maintenance.title}</TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(maintenance.start_at).toLocaleString()}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(maintenance.end_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <Badge variant={maintenance.active ? 'default' : 'secondary'}>
                      {maintenance.active ? t('common:enable') : t('common:disable')}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={maintenance.is_public ? 'default' : 'secondary'}>
                      {maintenance.is_public ? t('maintenance.is_public_yes') : t('maintenance.is_public_no')}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        onClick={() => {
                          setEditing(maintenance)
                          setDialogOpen(true)
                        }}
                        size="sm"
                        variant="ghost"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <AlertDialog
                        onOpenChange={(open) => {
                          if (!open) {
                            setDeleteId(null)
                          }
                        }}
                        open={deleteId === maintenance.id}
                      >
                        <AlertDialogTrigger
                          onClick={() => setDeleteId(maintenance.id)}
                          render={<Button disabled={deleteMutation.isPending} size="sm" variant="ghost" />}
                        >
                          <Trash2 className="size-3.5 text-destructive" />
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
                                deleteMutation.mutate(maintenance.id)
                                setDeleteId(null)
                              }}
                              variant="destructive"
                            >
                              {t('common:delete')}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <StatusPageMaintenanceFormDialog
        editing={editing}
        onClose={() => {
          setDialogOpen(false)
          setEditing(null)
        }}
        onSubmit={handleSubmit}
        open={dialogOpen}
        pending={createMutation.isPending || updateMutation.isPending}
        servers={servers}
      />
    </div>
  )
}
