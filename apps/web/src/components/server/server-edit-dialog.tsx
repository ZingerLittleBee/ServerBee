import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { api } from '@/lib/api-client'
import type { ServerGroup, ServerResponse, UpdateServerInput } from '@/lib/api-schema'

interface ServerEditDialogProps {
  onClose: () => void
  open: boolean
  server: ServerResponse
}

export function ServerEditDialog({ server, open, onClose }: ServerEditDialogProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [name, setName] = useState(server.name)
  const [weight, setWeight] = useState(server.weight)
  const [hidden, setHidden] = useState(server.hidden)
  const [groupId, setGroupId] = useState(server.group_id ?? '')
  const [remark, setRemark] = useState(server.remark ?? '')
  const [publicRemark, setPublicRemark] = useState(server.public_remark ?? '')
  const [price, setPrice] = useState(server.price?.toString() ?? '')
  const [billingCycle, setBillingCycle] = useState(server.billing_cycle ?? '')
  const [currency, setCurrency] = useState(server.currency ?? 'USD')
  const [expiredAt, setExpiredAt] = useState(server.expired_at?.slice(0, 10) ?? '')
  const [trafficLimit, setTrafficLimit] = useState(
    server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : ''
  )
  const [trafficLimitType, setTrafficLimitType] = useState(server.traffic_limit_type ?? 'sum')

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: open
  })

  useEffect(() => {
    if (open) {
      setName(server.name)
      setWeight(server.weight)
      setHidden(server.hidden)
      setGroupId(server.group_id ?? '')
      setRemark(server.remark ?? '')
      setPublicRemark(server.public_remark ?? '')
      setPrice(server.price?.toString() ?? '')
      setBillingCycle(server.billing_cycle ?? '')
      setCurrency(server.currency ?? 'USD')
      setExpiredAt(server.expired_at?.slice(0, 10) ?? '')
      setTrafficLimit(server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : '')
      setTrafficLimitType(server.traffic_limit_type ?? 'sum')
    }
  }, [open, server])

  const mutation = useMutation({
    mutationFn: (payload: UpdateServerInput) => api.put<ServerResponse>(`/api/servers/${server.id}`, payload),
    onSuccess: (data) => {
      queryClient.setQueryData(['servers', server.id], data)
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    }
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    const payload: UpdateServerInput = {
      name,
      weight,
      hidden,
      group_id: groupId || null,
      remark: remark || null,
      public_remark: publicRemark || null,
      price: price ? Number.parseFloat(price) : null,
      billing_cycle: billingCycle || null,
      currency: currency || null,
      expired_at: expiredAt ? `${expiredAt}T00:00:00Z` : null,
      traffic_limit: trafficLimit ? Math.round(Number.parseFloat(trafficLimit) * 1024 ** 3) : null,
      traffic_limit_type: trafficLimitType || null
    }
    mutation.mutate(payload, {
      onSuccess: () => {
        toast.success(t('edit_success', { defaultValue: 'Server updated successfully' }))
        onClose()
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : t('edit_failed'))
      }
    })
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
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg" style={{ overscrollBehavior: 'contain' }}>
        <DialogHeader>
          <DialogTitle>{t('edit_title')}</DialogTitle>
        </DialogHeader>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {/* Basic */}
          <fieldset className="space-y-3">
            <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
              {t('edit_basic')}
            </legend>
            <Field label={t('edit_name')}>
              <Input
                aria-label={t('edit_name')}
                name="name"
                onChange={(e) => setName(e.target.value)}
                required
                type="text"
                value={name}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_weight')}>
                <Input
                  aria-label={t('edit_weight')}
                  autoComplete="off"
                  name="weight"
                  onChange={(e) => setWeight(Number.parseInt(e.target.value, 10) || 0)}
                  type="number"
                  value={weight}
                />
              </Field>
              <Field label={t('edit_hidden')}>
                {/* biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element */}
                <label className="flex cursor-pointer items-center gap-2 pt-1">
                  <Checkbox checked={hidden} onCheckedChange={(checked) => setHidden(!!checked)} />
                  <span className="text-sm">{t('edit_hide_status')}</span>
                </label>
              </Field>
            </div>
            <Field label={t('edit_group')}>
              <Select
                onValueChange={(v) => setGroupId(v === '__none__' || v === null ? '' : v)}
                value={groupId || '__none__'}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">{t('edit_no_group')}</SelectItem>
                  {groups?.map((g) => (
                    <SelectItem key={g.id} value={g.id}>
                      {g.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label={t('edit_remark')}>
              <Input
                aria-label={t('edit_remark')}
                name="remark"
                onChange={(e) => setRemark(e.target.value)}
                placeholder={t('edit_remark_placeholder')}
                type="text"
                value={remark}
              />
            </Field>
            <Field label={t('edit_public_remark')}>
              <Input
                aria-label={t('edit_public_remark')}
                name="public_remark"
                onChange={(e) => setPublicRemark(e.target.value)}
                placeholder={t('edit_public_remark_placeholder')}
                type="text"
                value={publicRemark}
              />
            </Field>
          </fieldset>

          {/* Billing */}
          <fieldset className="space-y-3">
            <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
              {t('edit_billing')}
            </legend>
            <div className="grid grid-cols-3 gap-3">
              <Field label={t('edit_price')}>
                <Input
                  aria-label={t('edit_price')}
                  autoComplete="off"
                  min="0"
                  name="price"
                  onChange={(e) => setPrice(e.target.value)}
                  placeholder="0.00"
                  step="0.01"
                  type="number"
                  value={price}
                />
              </Field>
              <Field label={t('edit_currency')}>
                <Select onValueChange={(v) => v !== null && setCurrency(v)} value={currency}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="USD">USD</SelectItem>
                    <SelectItem value="EUR">EUR</SelectItem>
                    <SelectItem value="CNY">CNY</SelectItem>
                    <SelectItem value="JPY">JPY</SelectItem>
                    <SelectItem value="GBP">GBP</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Field label={t('edit_billing_cycle')}>
                <Select
                  onValueChange={(v) => setBillingCycle(v === '__none__' || v === null ? '' : v)}
                  value={billingCycle || '__none__'}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">{t('edit_none')}</SelectItem>
                    <SelectItem value="monthly">{t('edit_monthly')}</SelectItem>
                    <SelectItem value="quarterly">{t('edit_quarterly')}</SelectItem>
                    <SelectItem value="yearly">{t('edit_yearly')}</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <Field label={t('edit_expiration')}>
              <Input
                aria-label={t('edit_expiration')}
                name="expiration"
                onChange={(e) => setExpiredAt(e.target.value)}
                type="date"
                value={expiredAt}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_traffic_limit')}>
                <Input
                  aria-label={t('edit_traffic_limit')}
                  autoComplete="off"
                  min="0"
                  name="traffic_limit"
                  onChange={(e) => setTrafficLimit(e.target.value)}
                  placeholder={t('edit_unlimited')}
                  step="0.1"
                  type="number"
                  value={trafficLimit}
                />
              </Field>
              <Field label={t('edit_limit_type')}>
                <Select onValueChange={(v) => v !== null && setTrafficLimitType(v)} value={trafficLimitType}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="sum">{t('edit_total_in_out')}</SelectItem>
                    <SelectItem value="up">{t('edit_upload_only')}</SelectItem>
                    <SelectItem value="down">{t('edit_download_only')}</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </div>
          </fieldset>

          {mutation.error && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm">
              {mutation.error.message || t('edit_failed')}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <Button onClick={onClose} type="button" variant="outline">
              {t('common:cancel')}
            </Button>
            <Button disabled={mutation.isPending} type="submit">
              {mutation.isPending ? t('common:saving') : t('common:save')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function Field({ label, children }: { children: React.ReactNode; label: string }) {
  return (
    <div className="space-y-1">
      {/* biome-ignore lint/a11y/noLabelWithoutControl: label wraps child input via adjacent sibling pattern */}
      <label className="font-medium text-sm">{label}</label>
      {children}
    </div>
  )
}
