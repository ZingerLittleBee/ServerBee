import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'

export function IpQualitySettingsTab({
  defaultIntervalHours,
  isAdmin,
  onSubmit,
  updatePending
}: {
  defaultIntervalHours: number
  isAdmin: boolean
  onSubmit: (e: FormEvent<HTMLFormElement>) => void
  updatePending: boolean
}) {
  const { t } = useTranslation('ip-quality')

  return (
    <ScrollArea className="h-full">
      <form className="max-w-xl space-y-6 pb-1" onSubmit={onSubmit}>
        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="check-interval">
            {t('settings_check_interval')}
          </label>
          <Input
            autoComplete="off"
            defaultValue={defaultIntervalHours}
            disabled={!isAdmin}
            id="check-interval"
            key={defaultIntervalHours}
            max={168}
            min={1}
            name="check-interval"
            type="number"
          />
          <p className="text-muted-foreground text-xs">{t('settings_check_interval_hint')}</p>
        </div>

        <Button disabled={!isAdmin || updatePending} size="sm" type="submit">
          {t('settings_save')}
        </Button>
      </form>
    </ScrollArea>
  )
}
