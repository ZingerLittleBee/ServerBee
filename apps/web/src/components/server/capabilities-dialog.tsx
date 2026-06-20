import { Shield } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { TemporaryBadge } from '@/components/server/temporary-badge'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Separator } from '@/components/ui/separator'
import { useAuth } from '@/hooks/use-auth'
import { CAPABILITIES, classifyCapability, temporaryGrantFor } from '@/lib/capabilities'

interface ServerWithCaps {
  agent_local_capabilities?: number | null
  capabilities?: number | null
  effective_capabilities?: number | null
  id: string
  protocol_version?: number | null
  temporary?: Array<{ cap: string; expires_at: number; granted_at: number }> | null
}

export function CapabilitiesDialog({ server }: { server: ServerWithCaps }) {
  const { t } = useTranslation('servers')
  const { user } = useAuth()
  const [open, setOpen] = useState(false)

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
          defaultValue: 'Monitoring and maintenance features that are generally safe to keep enabled by default.'
        }),
        items: CAPABILITIES.filter(({ risk }) => risk !== 'high'),
        key: 'low',
        title: t('cap_group_low_risk', { defaultValue: 'Monitoring & Maintenance' })
      }
    ],
    [t]
  )

  if (user?.role !== 'admin') {
    return null
  }

  return (
    <>
      <Button onClick={() => setOpen(true)} size="sm" variant="outline">
        <Shield data-icon="inline-start" />
        {t('detail_capabilities', { defaultValue: 'Capabilities' })}
      </Button>

      {open && (
        <Dialog onOpenChange={setOpen} open={open}>
          <DialogContent className="sm:max-w-2xl">
            <DialogHeader>
              <DialogTitle>{t('cap_toggles')}</DialogTitle>
              <DialogDescription>
                {t('cap_dialog_description', {
                  defaultValue: 'Capabilities are configured in the agent config file and cannot be changed here.'
                })}
              </DialogDescription>
            </DialogHeader>

            <DialogBody className="space-y-4">
              <div className="rounded-lg border border-border bg-muted/40 p-3 text-muted-foreground text-sm">
                {t('cap_read_only_note', {
                  defaultValue:
                    'These capabilities are owned by the agent host. To change them, edit the [capabilities] section of the agent config file and restart the agent.'
                })}
              </div>

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
                        const state = classifyCapability(server, capability.bit)

                        return (
                          <div className="flex flex-col gap-3" key={capability.bit}>
                            {index > 0 && <Separator />}
                            <div className="flex items-center justify-between gap-4">
                              <div className="min-w-0 flex-1">
                                <div className="flex flex-wrap items-center gap-2">
                                  <span className="font-medium">{t(capability.labelKey)}</span>
                                  <Badge variant={capability.risk === 'high' ? 'destructive' : 'secondary'}>
                                    {capability.risk === 'high' ? t('cap_high_risk') : t('cap_low_risk')}
                                  </Badge>
                                </div>
                              </div>
                              {(() => {
                                if (state === 'temporary') {
                                  const grant = temporaryGrantFor(server, capability.bit)
                                  return <TemporaryBadge expiresAt={grant?.expires_at ?? null} />
                                }
                                if (state === 'enabled') {
                                  return (
                                    <Badge className="border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
                                      {t('cap_enabled', { defaultValue: 'Enabled' })}
                                    </Badge>
                                  )
                                }
                                return (
                                  <Badge className="text-muted-foreground" variant="outline">
                                    {t('cap_disabled', { defaultValue: 'Disabled' })}
                                  </Badge>
                                )
                              })()}
                            </div>
                          </div>
                        )
                      })}
                    </CardContent>
                  </Card>
                ))}
              </div>
            </DialogBody>
          </DialogContent>
        </Dialog>
      )}
    </>
  )
}
