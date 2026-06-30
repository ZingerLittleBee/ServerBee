import { useMemo, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { getNetworkProbeTypeLabel } from '@/lib/network-i18n'
import type { NetworkProbeTarget } from '@/lib/network-types'

export type ProbeType = 'icmp' | 'tcp' | 'http'

export interface TargetFormData {
  location: string
  name: string
  probe_type: ProbeType
  provider: string
  target: string
}

function getInitialProbeType(target: NetworkProbeTarget | null): ProbeType {
  if (target?.probe_type === 'tcp' || target?.probe_type === 'http') {
    return target.probe_type
  }

  return 'icmp'
}

function selectedProbeTypeReducer(_current: ProbeType, next: ProbeType): ProbeType {
  return next
}

export function NetworkProbeTargetDialog({
  createPending,
  onClose,
  onSubmit,
  open,
  target,
  updatePending
}: {
  createPending: boolean
  onClose: () => void
  onSubmit: (form: TargetFormData) => void
  open: boolean
  target: NetworkProbeTarget | null
  updatePending: boolean
}) {
  const { t } = useTranslation('network')
  const [probeType, setProbeType] = useReducer(selectedProbeTypeReducer, target, getInitialProbeType)
  const probeTypes = useMemo(
    () =>
      [
        { value: 'icmp', label: getNetworkProbeTypeLabel(t, 'icmp') },
        { value: 'tcp', label: getNetworkProbeTypeLabel(t, 'tcp') },
        { value: 'http', label: getNetworkProbeTypeLabel(t, 'http') }
      ] satisfies { label: string; value: ProbeType }[],
    [t]
  )

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{target ? t('edit_target') : t('add_target')}</DialogTitle>
        </DialogHeader>
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault()
            const formData = new FormData(event.currentTarget)
            const name = String(formData.get('target-name') ?? '')
            const targetAddress = String(formData.get('target-address') ?? '')

            if (!(name.trim() && targetAddress.trim())) {
              return
            }

            onSubmit({
              location: String(formData.get('target-location') ?? ''),
              name,
              probe_type: probeType,
              provider: String(formData.get('target-provider') ?? ''),
              target: targetAddress
            })
          }}
        >
          <div className="space-y-1">
            <label className="font-medium text-sm" htmlFor="form-name">
              {t('target_name')}
            </label>
            <Input
              autoComplete="off"
              defaultValue={target?.name ?? ''}
              id="form-name"
              name="target-name"
              placeholder={t('target_name')}
              required
              type="text"
            />
          </div>
          <div className="space-y-1">
            <label className="font-medium text-sm" htmlFor="form-provider">
              {t('target_provider')}
            </label>
            <Input
              autoComplete="off"
              defaultValue={target?.provider ?? ''}
              id="form-provider"
              name="target-provider"
              placeholder={t('target_provider')}
              type="text"
            />
          </div>
          <div className="space-y-1">
            <label className="font-medium text-sm" htmlFor="form-location">
              {t('target_location')}
            </label>
            <Input
              autoComplete="off"
              defaultValue={target?.location ?? ''}
              id="form-location"
              name="target-location"
              placeholder={t('target_location')}
              type="text"
            />
          </div>
          <div className="space-y-1">
            <label className="font-medium text-sm" htmlFor="form-target">
              {t('target_address')}
            </label>
            <Input
              autoComplete="off"
              defaultValue={target?.target ?? ''}
              id="form-target"
              name="target-address"
              placeholder={t('target_address_placeholder', { defaultValue: 'e.g. 1.1.1.1 or example.com:80' })}
              required
              type="text"
            />
          </div>
          <div className="space-y-1">
            <label className="font-medium text-sm" htmlFor="form-probe-type">
              {t('target_type')}
            </label>
            <Select items={probeTypes} onValueChange={(value) => setProbeType(value as ProbeType)} value={probeType}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {probeTypes.map((type) => (
                  <SelectItem key={type.value} value={type.value}>
                    {type.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex gap-2 pt-2">
            <Button disabled={createPending || updatePending} size="sm" type="submit">
              {target ? t('save') : t('add_target')}
            </Button>
            <Button onClick={onClose} size="sm" type="button" variant="ghost">
              {t('cancel')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
