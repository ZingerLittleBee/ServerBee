import { useMutation } from '@tanstack/react-query'
import { Loader2, Plus, RefreshCw } from 'lucide-react'
import QRCode from 'qrcode'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { api } from '@/lib/api-client'

interface PairResponse {
  code: string
  expires_in_secs: number
}

export function MobilePairDialog({ onPaired }: { onPaired?: () => void }) {
  const { t } = useTranslation(['settings', 'common'])
  const [open, setOpen] = useState(false)
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null)
  const [secondsLeft, setSecondsLeft] = useState(0)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current)
      timerRef.current = null
    }
  }, [])

  const pairMutation = useMutation({
    mutationFn: () => api.post<PairResponse>('/api/mobile/pair'),
    onSuccess: async (data) => {
      const qrPayload = JSON.stringify({
        type: 'serverbee_pair',
        server_url: window.location.origin,
        code: data.code
      })

      try {
        const dataUrl = await QRCode.toDataURL(qrPayload, {
          width: 256,
          margin: 2,
          color: { dark: '#000000', light: '#ffffff' }
        })
        setQrDataUrl(dataUrl)
      } catch {
        toast.error(t('mobile.qr_generation_failed'))
        return
      }

      setSecondsLeft(data.expires_in_secs)
      clearTimer()
      timerRef.current = setInterval(() => {
        setSecondsLeft((prev) => {
          if (prev <= 1) {
            clearTimer()
            return 0
          }
          return prev - 1
        })
      }, 1000)
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to generate pairing code')
    }
  })

  const handleOpen = (isOpen: boolean) => {
    setOpen(isOpen)
    if (isOpen) {
      setQrDataUrl(null)
      setSecondsLeft(0)
      pairMutation.mutate()
    } else {
      clearTimer()
      setQrDataUrl(null)
      setSecondsLeft(0)
      onPaired?.()
    }
  }

  useEffect(() => {
    return () => clearTimer()
  }, [clearTimer])

  const expired = secondsLeft === 0 && qrDataUrl !== null && !pairMutation.isPending
  const minutes = Math.floor(secondsLeft / 60)
  const seconds = secondsLeft % 60

  return (
    <Dialog onOpenChange={handleOpen} open={open}>
      <DialogTrigger render={<Button size="sm" />}>
        <Plus className="size-4" />
        {t('mobile.add_device')}
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('mobile.pair_title')}</DialogTitle>
          <DialogDescription>{t('mobile.pair_description')}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col items-center gap-4 py-4">
          {pairMutation.isPending && (
            <div className="flex size-64 items-center justify-center">
              <Loader2 className="size-8 animate-spin text-muted-foreground" />
            </div>
          )}

          {qrDataUrl && !pairMutation.isPending && (
            <>
              <div className="rounded-lg border bg-white p-2">
                <img alt={t('mobile.qr_alt')} className="size-60" height={240} src={qrDataUrl} width={240} />
              </div>
              {expired ? (
                <p className="text-destructive text-sm">{t('mobile.code_expired')}</p>
              ) : (
                <p className="text-muted-foreground text-sm">
                  {t('mobile.expires_in', {
                    time: `${minutes.toString()}:${seconds.toString().padStart(2, '0')}`
                  })}
                </p>
              )}
            </>
          )}
        </div>

        <DialogFooter>
          {expired && (
            <Button disabled={pairMutation.isPending} onClick={() => pairMutation.mutate()} variant="outline">
              <RefreshCw className="size-4" />
              {t('mobile.regenerate')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
