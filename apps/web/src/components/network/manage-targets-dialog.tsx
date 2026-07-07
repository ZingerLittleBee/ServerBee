import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  getNetworkProbeTypeLabel,
  getNetworkTargetDisplayLocation,
  getNetworkTargetDisplayName,
  getNetworkTargetDisplayProvider
} from '@/lib/network-i18n'
import type { NetworkProbeTarget } from '@/lib/network-types'

export interface ManageTargetsDialogProps {
  allTargets: NetworkProbeTarget[]
  isPending: boolean
  onDeselectAll: () => void
  onOpenChange: (open: boolean) => void
  onSave: () => void
  onSelectAll: () => void
  onToggleTarget: (id: string) => void
  open: boolean
  selectedTargetIds: Set<string>
}

export function ManageTargetsDialog({
  allTargets,
  isPending,
  onDeselectAll,
  onOpenChange,
  onSave,
  onSelectAll,
  onToggleTarget,
  open,
  selectedTargetIds
}: ManageTargetsDialogProps) {
  const { i18n, t } = useTranslation('network')
  const language = i18n.resolvedLanguage ?? i18n.language

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg" showCloseButton={false}>
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle>{t('manage_targets')}</DialogTitle>
            <div className="flex gap-2">
              <Button onClick={onSelectAll} size="sm" type="button" variant="ghost">
                {t('select_all')}
              </Button>
              <Button onClick={onDeselectAll} size="sm" type="button" variant="ghost">
                {t('deselect_all')}
              </Button>
            </div>
          </div>
        </DialogHeader>

        {allTargets.length === 0 ? (
          <p className="py-4 text-center text-muted-foreground text-sm">{t('no_targets')}</p>
        ) : (
          <ScrollArea className="max-h-[70vh] rounded-md border">
            <div className="space-y-1.5 p-3">
              {allTargets.map((target) => (
                // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                <label
                  className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 text-sm hover:bg-muted/40"
                  key={target.id}
                >
                  <Checkbox
                    checked={selectedTargetIds.has(target.id)}
                    onCheckedChange={() => onToggleTarget(target.id)}
                  />
                  <span className="flex-1 font-medium">{getNetworkTargetDisplayName(t, language, target)}</span>
                  {target.provider && (
                    <span className="text-muted-foreground text-xs">
                      {getNetworkTargetDisplayProvider(t, language, target)}
                    </span>
                  )}
                  {target.location && (
                    <span className="text-muted-foreground text-xs">
                      {getNetworkTargetDisplayLocation(t, language, target)}
                    </span>
                  )}
                  <span className="rounded-full bg-muted px-2 py-0.5 text-xs">
                    {getNetworkProbeTypeLabel(t, target.probe_type)}
                  </span>
                </label>
              ))}
            </div>
          </ScrollArea>
        )}

        <div className="flex gap-2">
          <Button disabled={isPending} onClick={onSave} size="sm">
            {t('save')}
          </Button>
          <DialogClose render={<Button size="sm" variant="ghost" />}>{t('cancel')}</DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  )
}
