import { Loader2, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { useCancelTransferMutation, useFileTransfers } from '@/hooks/use-file-api'
import { formatBytes } from '@/lib/utils'

function statusLabel(status: string, t: (key: string) => string): string {
  if (status === 'pending') {
    return t('transfer_pending')
  }
  if (status === 'in_progress') {
    return t('transfer_in_progress')
  }
  if (status === 'ready') {
    return t('transfer_complete')
  }
  if (status === 'failed') {
    return t('transfer_failed')
  }
  return status
}

export function TransferBar() {
  const { t } = useTranslation('file')
  const { data: transfers } = useFileTransfers()
  const cancelMutation = useCancelTransferMutation()

  if (!transfers || transfers.length === 0) {
    return null
  }

  return (
    <div className="border-t bg-muted/30 px-3 py-2">
      <div className="space-y-1.5">
        {transfers.map((transfer) => {
          const progress =
            transfer.file_size && transfer.file_size > 0
              ? Math.min(100, Math.round((transfer.bytes_transferred / transfer.file_size) * 100))
              : 0
          const fileName = transfer.file_path.split('/').pop() ?? transfer.file_path

          return (
            <div className="flex items-center gap-2 text-xs" key={transfer.transfer_id}>
              {transfer.status === 'in_progress' && <Loader2 className="size-3 animate-spin text-muted-foreground" />}
              <span className="min-w-0 flex-1 truncate" title={transfer.file_path}>
                {fileName}
              </span>
              <span className="text-muted-foreground">
                {formatBytes(transfer.bytes_transferred)}
                {transfer.file_size ? ` / ${formatBytes(transfer.file_size)}` : ''}
              </span>
              {transfer.file_size && transfer.file_size > 0 && (
                <div className="h-1.5 w-20 overflow-hidden rounded-full bg-muted">
                  <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
                </div>
              )}
              <span className="text-muted-foreground">{statusLabel(transfer.status, t)}</span>
              {(transfer.status === 'pending' || transfer.status === 'in_progress') && (
                <Button
                  disabled={cancelMutation.isPending}
                  onClick={() => cancelMutation.mutate(transfer.transfer_id)}
                  size="icon-xs"
                  title={t('cancel_transfer')}
                  variant="ghost"
                >
                  <X className="size-3" />
                </Button>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
