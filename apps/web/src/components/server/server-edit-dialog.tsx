import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
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
      onClose()
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
    mutation.mutate(payload)
  }

  if (!open) {
    return null
  }

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: modal backdrop dismissal pattern
    // biome-ignore lint/a11y/noStaticElementInteractions: modal backdrop dismissal pattern
    // biome-ignore lint/a11y/noNoninteractiveElementInteractions: modal backdrop dismissal pattern
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: stop propagation for modal content */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: stop propagation for modal content */}
      {/* biome-ignore lint/a11y/noNoninteractiveElementInteractions: stop propagation for modal content */}
      <div
        className="relative max-h-[85vh] w-full max-w-lg overflow-y-auto rounded-lg border bg-background p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          className="absolute top-4 right-4 text-muted-foreground hover:text-foreground"
          onClick={onClose}
          type="button"
        >
          <X className="size-4" />
        </button>

        <h2 className="mb-4 font-semibold text-lg">{t('edit_title')}</h2>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {/* Basic */}
          <fieldset className="space-y-3">
            <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
              {t('edit_basic')}
            </legend>
            <Field label={t('edit_name')}>
              <input
                className="input-field"
                onChange={(e) => setName(e.target.value)}
                required
                type="text"
                value={name}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_weight')}>
                <input
                  className="input-field"
                  onChange={(e) => setWeight(Number.parseInt(e.target.value, 10) || 0)}
                  type="number"
                  value={weight}
                />
              </Field>
              <Field label={t('edit_hidden')}>
                <label className="flex cursor-pointer items-center gap-2 pt-1">
                  <input
                    checked={hidden}
                    className="size-4 rounded border accent-primary"
                    onChange={(e) => setHidden(e.target.checked)}
                    type="checkbox"
                  />
                  <span className="text-sm">{t('edit_hide_status')}</span>
                </label>
              </Field>
            </div>
            <Field label={t('edit_group')}>
              <select className="input-field" onChange={(e) => setGroupId(e.target.value)} value={groupId}>
                <option value="">{t('edit_no_group')}</option>
                {groups?.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t('edit_remark')}>
              <input
                className="input-field"
                onChange={(e) => setRemark(e.target.value)}
                placeholder={t('edit_remark_placeholder')}
                type="text"
                value={remark}
              />
            </Field>
            <Field label={t('edit_public_remark')}>
              <input
                className="input-field"
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
                <input
                  className="input-field"
                  min="0"
                  onChange={(e) => setPrice(e.target.value)}
                  placeholder="0.00"
                  step="0.01"
                  type="number"
                  value={price}
                />
              </Field>
              <Field label={t('edit_currency')}>
                <select className="input-field" onChange={(e) => setCurrency(e.target.value)} value={currency}>
                  <option value="USD">USD</option>
                  <option value="EUR">EUR</option>
                  <option value="CNY">CNY</option>
                  <option value="JPY">JPY</option>
                  <option value="GBP">GBP</option>
                </select>
              </Field>
              <Field label={t('edit_billing_cycle')}>
                <select className="input-field" onChange={(e) => setBillingCycle(e.target.value)} value={billingCycle}>
                  <option value="">{t('edit_none')}</option>
                  <option value="monthly">{t('edit_monthly')}</option>
                  <option value="quarterly">{t('edit_quarterly')}</option>
                  <option value="yearly">{t('edit_yearly')}</option>
                </select>
              </Field>
            </div>
            <Field label={t('edit_expiration')}>
              <input
                className="input-field"
                onChange={(e) => setExpiredAt(e.target.value)}
                type="date"
                value={expiredAt}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_traffic_limit')}>
                <input
                  className="input-field"
                  min="0"
                  onChange={(e) => setTrafficLimit(e.target.value)}
                  placeholder={t('edit_unlimited')}
                  step="0.1"
                  type="number"
                  value={trafficLimit}
                />
              </Field>
              <Field label={t('edit_limit_type')}>
                <select
                  className="input-field"
                  onChange={(e) => setTrafficLimitType(e.target.value)}
                  value={trafficLimitType}
                >
                  <option value="sum">{t('edit_total_in_out')}</option>
                  <option value="up">{t('edit_upload_only')}</option>
                  <option value="down">{t('edit_download_only')}</option>
                </select>
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

        <style>{`
          .input-field {
            display: flex;
            height: 2.25rem;
            width: 100%;
            border-radius: 0.375rem;
            border: 1px solid var(--color-border);
            background: transparent;
            padding: 0.25rem 0.75rem;
            font-size: 0.875rem;
            box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05);
            transition: box-shadow 0.15s, border-color 0.15s;
          }
          .input-field:focus-visible {
            outline: none;
            ring: 1px;
            box-shadow: 0 0 0 1px var(--color-ring);
            border-color: var(--color-ring);
          }
        `}</style>
      </div>
    </div>
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
