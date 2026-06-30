import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/api-client'
import { INCIDENT_STATUSES } from './status-page-incident-options'

export function StatusPageIncidentUpdateDialog({
  incidentId,
  onClose,
  open
}: {
  incidentId: string
  onClose: () => void
  open: boolean
}) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [message, setMessage] = useState('')
  const [status, setStatus] = useState<string>('investigating')

  const addUpdateMutation = useMutation({
    mutationFn: (input: { message: string; status: string }) => api.post(`/api/incidents/${incidentId}/updates`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
      onClose()
      setMessage('')
      toast.success(t('incidents.update_added'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!message.trim()) {
      return
    }
    addUpdateMutation.mutate({ message: message.trim(), status })
  }

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('incidents.add_update')}</DialogTitle>
          <DialogDescription>{t('incidents.add_update_description')}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="incident-update-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="upd-status">{t('incidents.field_status')}</Label>
            <Select onValueChange={(value) => value && setStatus(value)} value={status}>
              <SelectTrigger id="upd-status">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INCIDENT_STATUSES.map((statusValue) => (
                  <SelectItem key={statusValue} value={statusValue}>
                    {t(`incidents.status_${statusValue}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label htmlFor="upd-message">{t('incidents.field_message')}</Label>
            <Textarea
              id="upd-message"
              onChange={(e) => setMessage(e.target.value)}
              placeholder={t('incidents.placeholder_message')}
              required
              rows={3}
              value={message}
            />
          </div>
        </form>
        <DialogFooter>
          <Button disabled={addUpdateMutation.isPending} form="incident-update-form" type="submit">
            {t('incidents.post_update')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
