import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Shield } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Separator } from '@/components/ui/separator'
import { Switch } from '@/components/ui/switch'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import { CAP_DEFAULT, CAPABILITIES, getEffectiveCapabilityEnabled, isClientCapabilityLocked } from '@/lib/capabilities'

interface ServerWithCaps {
  agent_local_capabilities?: number | null
  capabilities?: number | null
  effective_capabilities?: number | null
  id: string
  protocol_version?: number | null
}

const HIGH_RISK_KEYS = new Set(['exec', 'file', 'terminal', 'upgrade'])

export function CapabilitiesDialog({ server }: { server: ServerWithCaps }) {
  const { t } = useTranslation('servers')
  const { user } = useAuth()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)

  const mutation = useMutation({
    mutationFn: (newCaps: number) => api.put(`/api/servers/${server.id}`, { capabilities: newCaps }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', server.id] })
    }
  })

  const capabilityGroups = useMemo(
    () => [
      {
        description: t('cap_group_high_risk_desc', {
          defaultValue: 'Powerful controls that can change system state or expose shell and file access.'
        }),
        items: CAPABILITIES.filter(({ risk }) => risk === 'high'),
        key: 'high',
        title: t('cap_group_high_risk', { defaultValue: 'High Risk Operations' })
      },
      {
        description: t('cap_group_low_risk_desc', {
          defaultValue: 'Network checks and observability features that are generally safe to keep enabled.'
        }),
        items: CAPABILITIES.filter(({ risk }) => risk === 'low'),
        key: 'low',
        title: t('cap_group_low_risk', { defaultValue: 'Monitoring & Maintenance' })
      }
    ],
    [t]
  )

  if (user?.role !== 'admin') {
    return null
  }

  const caps = server.capabilities ?? CAP_DEFAULT

  const toggle = (bit: number) => {
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
    const newCaps = caps & bit ? caps & ~bit : caps | bit
    mutation.mutate(newCaps, {
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : 'Operation failed')
      },
      onSuccess: () => {
        toast.success('Capabilities updated')
      }
    })
  }

  return (
    <>
      <Button onClick={() => setOpen(true)} size="sm" variant="outline">
        <Shield data-icon="inline-start" />
        {t('detail_capabilities', { defaultValue: 'Capabilities' })}
      </Button>

      {open && (
        <Dialog onOpenChange={setOpen} open={open}>
          <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
            <DialogHeader>
              <DialogTitle>{t('cap_toggles')}</DialogTitle>
              <DialogDescription>
                {t('cap_dialog_description', {
                  defaultValue: 'Control which agent capabilities are enabled for this server.'
                })}
              </DialogDescription>
            </DialogHeader>

            {server.protocol_version != null && server.protocol_version < 2 && (
              <div className="rounded-lg border border-amber-200 bg-amber-50/80 p-3 text-amber-900 text-sm dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300">
                {t('cap_upgrade_warning')}
              </div>
            )}

            <div className="grid gap-4 lg:grid-cols-2">
              {capabilityGroups.map((group) => (
                <Card key={group.key} size="sm">
                  <CardHeader>
                    <CardTitle>{group.title}</CardTitle>
                    <CardDescription>{group.description}</CardDescription>
                    <CardAction>
                      <Badge variant={group.key === 'high' ? 'destructive' : 'secondary'}>{group.items.length}</Badge>
                    </CardAction>
                  </CardHeader>
                  <CardContent className="flex flex-col gap-3">
                    {group.items.map((capability, index) => {
                      const isEnabled = getEffectiveCapabilityEnabled(
                        server.effective_capabilities,
                        caps,
                        capability.bit
                      )
                      const isLocked = isClientCapabilityLocked(server.agent_local_capabilities, capability.bit)

                      return (
                        <div className="flex flex-col gap-3" key={capability.bit}>
                          {index > 0 && <Separator />}
                          <div className="flex items-center justify-between gap-4">
                            <div className="min-w-0 flex-1">
                              <div className="flex flex-wrap items-center gap-2">
                                <span className="font-medium">{t(capability.labelKey)}</span>
                                <Badge variant={HIGH_RISK_KEYS.has(capability.key) ? 'destructive' : 'secondary'}>
                                  {capability.risk === 'high' ? t('cap_high_risk') : t('cap_low_risk')}
                                </Badge>
                              </div>
                              <div className="mt-1 text-muted-foreground text-xs">
                                {isEnabled
                                  ? t('cap_enabled', { defaultValue: 'Enabled' })
                                  : t('cap_disabled', { defaultValue: 'Disabled' })}
                              </div>
                            </div>
                            <Switch
                              checked={isEnabled}
                              disabled={mutation.isPending || isLocked}
                              onCheckedChange={() => toggle(capability.bit)}
                              title={isLocked ? '客户端关闭' : undefined}
                            />
                          </div>
                        </div>
                      )
                    })}
                  </CardContent>
                </Card>
              ))}
            </div>
          </DialogContent>
        </Dialog>
      )}
    </>
  )
}
