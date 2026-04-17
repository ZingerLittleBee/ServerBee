import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2, RotateCcw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'
import { startRecoveryMerge, useRecoveryCandidates } from '@/hooks/use-api'
import type { RecoveryJobResponse } from '@/lib/api-schema'

interface RecoveryMergeDialogProps {
  currentJob?: RecoveryJobResponse
  onOpenChange: (open: boolean) => void
  open: boolean
  targetServerId: string
}

export function RecoveryMergeDialog({ currentJob, onOpenChange, open, targetServerId }: RecoveryMergeDialogProps) {
  const { t } = useTranslation('servers')
  const queryClient = useQueryClient()
  const [selectedSourceId, setSelectedSourceId] = useState('')
  const readOnly = currentJob != null

  const candidatesQuery = useRecoveryCandidates(targetServerId, open && !readOnly)

  const startMutation = useMutation({
    mutationFn: (sourceServerId: string) => startRecoveryMerge(targetServerId, { source_server_id: sourceServerId }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', targetServerId, 'recovery-candidates'] })
      toast.success(t('recovery_merge_started', { defaultValue: 'Recovery started' }))
      onOpenChange(false)
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : t('recovery_merge_failed', { defaultValue: 'Recovery failed' })
      )
    }
  })

  const candidates = candidatesQuery.data ?? []
  const selectedCandidate = candidates.find((candidate) => candidate.server_id === selectedSourceId)
  const canSubmit = !readOnly && selectedCandidate != null && !startMutation.isPending

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('recovery_merge_title', { defaultValue: 'Recover Offline Server' })}</DialogTitle>
          <DialogDescription>
            {t('recovery_merge_description', {
              defaultValue: 'Pick the online replacement agent to rebind and merge back into this offline server.'
            })}
          </DialogDescription>
        </DialogHeader>

        <DialogBody className="space-y-4">
          {currentJob && (
            <div className="rounded-lg border bg-muted/50 p-3 text-sm">
              <div className="flex items-center gap-2">
                <Badge variant="secondary">{currentJob.stage}</Badge>
                <span>
                  {t('recovery_merge_existing_job', { defaultValue: 'A recovery job is already in progress.' })}
                </span>
              </div>
            </div>
          )}

          {candidatesQuery.isLoading && (
            <div className="flex items-center gap-2 rounded-lg border p-3 text-muted-foreground text-sm">
              <Loader2 className="size-4 animate-spin" />
              {t('recovery_merge_loading', { defaultValue: 'Loading recovery candidates…' })}
            </div>
          )}

          {candidatesQuery.isError && (
            <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-destructive text-sm">
              {t('recovery_merge_candidates_failed', { defaultValue: 'Failed to load recovery candidates.' })}
            </div>
          )}

          {!(candidatesQuery.isLoading || candidatesQuery.isError) && candidates.length === 0 && (
            <div className="rounded-lg border p-3 text-muted-foreground text-sm">
              {t('recovery_merge_empty', { defaultValue: 'No online recovery candidates are available right now.' })}
            </div>
          )}

          {readOnly ? (
            <div className="rounded-lg border bg-muted/30 p-3 text-muted-foreground text-sm">
              {t('recovery_merge_read_only', {
                defaultValue: 'This dialog is read-only while a recovery job is active.'
              })}
            </div>
          ) : (
            candidates.length > 0 && (
              <div className="space-y-3 rounded-lg border p-3">
                {candidates.map((candidate) => {
                  const selected = candidate.server_id === selectedSourceId
                  return (
                    <button
                      className={`w-full rounded-lg border p-3 text-left ${selected ? 'border-primary bg-primary/5' : 'border-border'}`}
                      disabled={readOnly}
                      key={candidate.server_id}
                      onClick={() => setSelectedSourceId(candidate.server_id)}
                      type="button"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div>
                          <div className="font-medium">{candidate.name}</div>
                          <div className="text-muted-foreground text-xs">{candidate.server_id}</div>
                        </div>
                        <Badge variant="secondary">{candidate.score}</Badge>
                      </div>
                      <div className="mt-2 flex flex-wrap gap-1">
                        {candidate.reasons.map((reason) => (
                          <Badge key={reason} variant="outline">
                            {reason}
                          </Badge>
                        ))}
                      </div>
                    </button>
                  )
                })}
              </div>
            )
          )}

          <div className="rounded-lg border bg-muted/30 p-3 text-muted-foreground text-sm">
            {t('recovery_merge_warning', {
              defaultValue:
                'This keeps the original server record, asks the replacement agent to rebind, and continues the recovery flow from there.'
            })}
          </div>
        </DialogBody>

        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} variant="outline">
            {t('common:cancel', { defaultValue: 'Cancel' })}
          </Button>
          <Button
            disabled={!canSubmit}
            onClick={() => selectedCandidate && startMutation.mutate(selectedCandidate.server_id)}
          >
            {startMutation.isPending ? (
              <>
                <Loader2 className="mr-2 size-4 animate-spin" />
                {t('recovery_merge_starting', { defaultValue: 'Starting…' })}
              </>
            ) : (
              <>
                <RotateCcw className="mr-2 size-4" />
                {t('recovery_merge_start', { defaultValue: 'Start Recovery' })}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
