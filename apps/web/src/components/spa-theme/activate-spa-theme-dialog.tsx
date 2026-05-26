import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { SpaThemeSummary } from '@/api/spa-themes'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'

interface Props {
  onConfirm: () => void
  onOpenChange: (v: boolean) => void
  open: boolean
  theme: SpaThemeSummary | null
}

export function ActivateSpaThemeDialog({ theme, open, onOpenChange, onConfirm }: Props) {
  const { t } = useTranslation('spa-theme')
  const [agreed, setAgreed] = useState(false)

  if (!theme) {
    return null
  }

  const handleOpenChange = (v: boolean) => {
    setAgreed(false)
    onOpenChange(v)
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('activate_dialog.title', { name: theme.name, version: theme.version })}</DialogTitle>
          <DialogDescription>{t('activate_dialog.body')}</DialogDescription>
        </DialogHeader>
        <p className="text-muted-foreground text-sm">{t('activate_dialog.recovery_hint')}</p>
        {/* biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element */}
        <label className="flex cursor-pointer items-center gap-2 text-sm">
          <Checkbox checked={agreed} onCheckedChange={(v) => setAgreed(v === true)} />
          {t('activate_dialog.confirm_checkbox')}
        </label>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} variant="outline">
            {t('activate_dialog.cancel')}
          </Button>
          <Button disabled={!agreed} onClick={onConfirm}>
            {t('activate_dialog.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
