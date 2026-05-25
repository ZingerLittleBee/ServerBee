import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Copy, RefreshCw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ApiError, api } from '@/lib/api-client'
import type { RegenerateCodeRequest, RegenerateCodeResponse } from '@/lib/api-schema'

interface RegenerateCodeDialogProps {
  onOpenChange: (open: boolean) => void
  open: boolean
  serverId: string
}

export function RegenerateCodeDialog({ open, onOpenChange, serverId }: RegenerateCodeDialogProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [issued, setIssued] = useState<RegenerateCodeResponse | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const autoFiredRef = useRef(false)

  const mutation = useMutation({
    mutationFn: (body: RegenerateCodeRequest) =>
      api.post<RegenerateCodeResponse>(`/api/servers/${serverId}/regenerate-code`, body),
    onSuccess: (data) => {
      setIssued(data)
      setErrorMessage(null)
      toast.success(t('servers:card_pending.regenerated'))
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    },
    onError: (err: unknown) => {
      const message =
        err instanceof ApiError || err instanceof Error ? err.message : t('servers:card_pending.regenerate_failed')
      setErrorMessage(message)
      toast.error(t('servers:card_pending.regenerate_failed'))
    }
  })

  const mutateRef = useRef(mutation.mutate)
  mutateRef.current = mutation.mutate

  // Auto-fire the regenerate request the first render after the dialog opens, and
  // reset bookkeeping when it closes so reopening produces a fresh one-shot.
  useEffect(() => {
    if (!open) {
      setIssued(null)
      setErrorMessage(null)
      autoFiredRef.current = false
      return
    }
    if (!autoFiredRef.current) {
      autoFiredRef.current = true
      mutateRef.current({})
    }
  }, [open])

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('servers:add_server.copied'))
    } catch {
      // Clipboard access denied; ignore.
    }
  }

  const retry = () => {
    setErrorMessage(null)
    mutation.mutate({})
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('servers:card_pending.regenerate_title')}</DialogTitle>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p className="text-muted-foreground text-sm">{t('servers:card_pending.regenerate_description')}</p>

          {issued && (
            <div className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-3">
              <p className="text-amber-600 text-xs dark:text-amber-500">{t('servers:add_server.shown_once_warning')}</p>
              <div className="flex min-w-0 items-center gap-2">
                <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                  {issued.enrollment.code}
                </code>
                <Button
                  aria-label={t('servers:add_server.copy')}
                  onClick={() => copy(issued.enrollment.code)}
                  size="icon"
                  type="button"
                  variant="outline"
                >
                  <Copy className="size-4" />
                </Button>
              </div>
            </div>
          )}

          {errorMessage && !issued && (
            <div className="space-y-2 rounded-md border border-red-500/40 bg-red-500/5 p-3 text-red-600 text-sm dark:text-red-400">
              <p>{errorMessage}</p>
              <Button disabled={mutation.isPending} onClick={retry} size="sm" type="button" variant="outline">
                <RefreshCw aria-hidden="true" className="size-3.5" />
                {t('servers:card_pending.regenerate_code')}
              </Button>
            </div>
          )}

          {!(issued || errorMessage) && mutation.isPending && (
            <p className="text-muted-foreground text-sm">{t('servers:add_server.generating')}</p>
          )}
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} type="button" variant="outline">
            {t('common:close', { defaultValue: 'Close' })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
