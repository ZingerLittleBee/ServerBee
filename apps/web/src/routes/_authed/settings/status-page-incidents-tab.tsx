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
import type { CreateIncidentRequest, IncidentItem, ServerResponse, UpdateIncidentRequest } from '@/lib/api-schema'
import { StatusPageIncidentFormDialog } from './status-page-incident-form-dialog'
import { StatusPageIncidentUpdateDialog } from './status-page-incident-update-dialog'

export function StatusPageIncidentsTab({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<IncidentItem | null>(null)
  const [updateDialogIncidentId, setUpdateDialogIncidentId] = useState<string | null>(null)
  const [deleteId, setDeleteId] = useState<string | null>(null)

  const { data: incidents, isLoading } = useQuery<IncidentItem[]>({
    queryKey: ['incidents'],
    queryFn: () => api.get<IncidentItem[]>('/api/incidents')
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateIncidentRequest) => api.post('/api/incidents', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
      setDialogOpen(false)
      toast.success(t('incidents.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateIncidentRequest }) => api.put(`/api/incidents/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('incidents.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/incidents/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
      toast.success(t('incidents.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (data: CreateIncidentRequest | UpdateIncidentRequest, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data as UpdateIncidentRequest })
    } else {
      createMutation.mutate(data as CreateIncidentRequest)
    }
  }

  return (
    <div>
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-muted-foreground text-sm">{t('incidents.tab_description')}</p>
        <Button
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="size-4" />
          {t('incidents.create')}
        </Button>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!isLoading && (!incidents || incidents.length === 0) && (
        <div className="rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">{t('incidents.empty')}</p>
        </div>
      )}

      {incidents && incidents.length > 0 && (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('incidents.field_title')}</TableHead>
                <TableHead>{t('incidents.field_severity')}</TableHead>
                <TableHead>{t('incidents.field_status')}</TableHead>
                <TableHead>{t('incidents.field_is_public')}</TableHead>
                <TableHead>{t('incidents.col_created')}</TableHead>
                <TableHead className="text-right">{t('status_pages.col_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {incidents.map((incident) => (
                <TableRow key={incident.id}>
                  <TableCell className="font-medium">{incident.title}</TableCell>
                  <TableCell>
                    <Badge variant={incident.severity === 'critical' ? 'destructive' : 'secondary'}>
                      {incident.severity}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={incident.status === 'resolved' ? 'default' : 'outline'}>{incident.status}</Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={incident.is_public ? 'default' : 'secondary'}>
                      {incident.is_public ? t('incidents.is_public_yes') : t('incidents.is_public_no')}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(incident.created_at).toLocaleDateString()}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        onClick={() => setUpdateDialogIncidentId(incident.id)}
                        size="sm"
                        title={t('incidents.add_update')}
                        variant="ghost"
                      >
                        <Plus className="size-3.5" />
                      </Button>
                      <Button
                        onClick={() => {
                          setEditing(incident)
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
                        open={deleteId === incident.id}
                      >
                        <AlertDialogTrigger
                          onClick={() => setDeleteId(incident.id)}
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
                                deleteMutation.mutate(incident.id)
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

      <StatusPageIncidentFormDialog
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

      {updateDialogIncidentId && (
        <StatusPageIncidentUpdateDialog
          incidentId={updateDialogIncidentId}
          onClose={() => setUpdateDialogIncidentId(null)}
          open={!!updateDialogIncidentId}
        />
      )}
    </div>
  )
}
