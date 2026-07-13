import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Copy, RefreshCw } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AgentCapabilityPicker,
  ALL_AGENT_CAPABILITY_KEYS,
  agentCapabilityKeysForMask,
  DEFAULT_AGENT_CAPABILITY_KEYS,
  resolveAgentCapabilitySelection
} from '@/components/server/agent-capability-picker'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ApiError, api } from '@/lib/api-client'
import type {
  AgentAuthorityStateSummary,
  EnrollmentOfferResponse,
  OutstandingEnrollmentSummary,
  ReenrollmentRequest,
  RevokeAuthorityResponse,
  RevokeOfferResponse,
  ServerResponse
} from '@/lib/api-schema'
import { projectServerCatalog } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'

function formatCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(remainingMs / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`
}

interface OutstandingNoticeProps {
  authorityStatus: AgentAuthorityStateSummary['status']
  enrollment: OutstandingEnrollmentSummary
  onClose: () => void
  onReplaced: (response: EnrollmentOfferResponse) => void
  serverId: string
}

function OutstandingNotice({ authorityStatus, enrollment, onClose, onReplaced, serverId }: OutstandingNoticeProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const expiresAt = new Date(enrollment.expires_at).getTime()
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (expiresAt <= Date.now()) {
      return
    }
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [expiresAt])

  const revokeMutation = useMutation({
    mutationFn: () =>
      api.delete<RevokeOfferResponse>(`/api/servers/${serverId}/agent-authority/offers/${enrollment.id}`),
    onSuccess: () => {
      toast.success(t('agent_reenrollment.offer_revoked'))
      projectServerCatalog(queryClient, {
        authority: { outstanding_offer: null, status: authorityStatus },
        kind: 'agent_authority_changed',
        serverId
      })
    },
    onError: (err: unknown) => {
      const message =
        err instanceof ApiError || err instanceof Error ? err.message : t('agent_reenrollment.revoke_offer_failed')
      toast.error(message)
    }
  })

  const replaceMutation = useMutation({
    mutationFn: () =>
      api.post<EnrollmentOfferResponse>(`/api/servers/${serverId}/agent-authority/offers/${enrollment.id}/replace`, {}),
    onSuccess: (response) => {
      onReplaced(response)
      projectServerCatalog(queryClient, {
        authority: {
          outstanding_offer: {
            id: response.enrollment.id,
            code_prefix: response.enrollment.code_prefix,
            created_at: new Date().toISOString(),
            expires_at: response.enrollment.expires_at
          },
          status: authorityStatus
        },
        kind: 'agent_authority_changed',
        serverId
      })
    },
    onError: (err: unknown) => {
      const message =
        err instanceof ApiError || err instanceof Error ? err.message : t('agent_reenrollment.replace_failed')
      toast.error(message)
    }
  })

  const countdownLabel =
    expiresAt > now
      ? t('card_pending.code_expires_in', {
          prefix: enrollment.code_prefix,
          countdown: formatCountdown(expiresAt - now)
        })
      : t('card_pending.code_expired', { prefix: enrollment.code_prefix })
  const isExpired = expiresAt <= now

  return (
    <>
      <DialogBody className="space-y-4">
        <div className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
          <p className="font-medium text-amber-700 text-sm dark:text-amber-400">
            {t(isExpired ? 'agent_reenrollment.expired_notice_title' : 'agent_reenrollment.outstanding_notice_title')}
          </p>
          <p className="font-mono text-amber-700 text-xs tabular-nums dark:text-amber-400">{enrollment.code_prefix}…</p>
          <p className="text-amber-700 text-xs tabular-nums dark:text-amber-400">{countdownLabel}</p>
          <p className="text-muted-foreground text-xs">
            {t(isExpired ? 'agent_reenrollment.expired_notice_body' : 'agent_reenrollment.outstanding_notice_body')}
          </p>
        </div>
      </DialogBody>
      <DialogFooter>
        <Button onClick={onClose} type="button" variant="outline">
          {t('common:close', { defaultValue: 'Close' })}
        </Button>
        {!isExpired && (
          <>
            <Button
              disabled={revokeMutation.isPending}
              onClick={() => revokeMutation.mutate()}
              type="button"
              variant="destructive"
            >
              <RefreshCw aria-hidden="true" className="size-3.5" />
              {t('agent_reenrollment.revoke_offer')}
            </Button>
            <Button disabled={replaceMutation.isPending} onClick={() => replaceMutation.mutate()} type="button">
              <RefreshCw aria-hidden="true" className="size-3.5" />
              {t('agent_reenrollment.replace_offer')}
            </Button>
          </>
        )}
      </DialogFooter>
    </>
  )
}

interface AgentReenrollmentDialogProps {
  onOpenChange: (open: boolean) => void
  open: boolean
  server: Pick<ServerResponse, 'id' | 'name' | 'capabilities' | 'agent_authority'>
}

function initialCapsFor(caps: number | null | undefined): Set<string> {
  return agentCapabilityKeysForMask(caps)
}

export function AgentReenrollmentDialog({ open, onOpenChange, server }: AgentReenrollmentDialogProps) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      {open && <AgentReenrollmentDialogContent key={server.id} onOpenChange={onOpenChange} server={server} />}
    </Dialog>
  )
}

function AgentReenrollmentDialogContent({
  onOpenChange,
  server
}: {
  onOpenChange: (open: boolean) => void
  server: Pick<ServerResponse, 'id' | 'name' | 'capabilities' | 'agent_authority'>
}) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()

  const [selectedCaps, setSelectedCaps] = useState<Set<string>>(() => initialCapsFor(server.capabilities))
  const [emergencyMode, setEmergencyMode] = useState(true)
  const [issued, setIssued] = useState<EnrollmentOfferResponse | null>(null)
  const [confirmRevokeOpen, setConfirmRevokeOpen] = useState(false)

  const mutation = useMutation({
    mutationFn: (body: ReenrollmentRequest) =>
      api.post<EnrollmentOfferResponse>(`/api/servers/${server.id}/agent-authority/re-enrollment`, body),
    onSuccess: (data, variables) => {
      setIssued(data)
      const revoked = variables.mode === 'emergency'
      const newOutstanding = {
        id: data.enrollment.id,
        code_prefix: data.enrollment.code_prefix,
        expires_at: data.enrollment.expires_at,
        created_at: new Date().toISOString()
      }
      projectServerCatalog(queryClient, {
        authority: {
          outstanding_offer: newOutstanding,
          status: revoked ? 'unclaimed' : 'claimed'
        },
        kind: 'agent_authority_changed',
        serverId: server.id
      })
    },
    onError: (err: unknown) => {
      const message =
        err instanceof ApiError || err instanceof Error ? err.message : t('agent_reenrollment.generate_failed')
      toast.error(message)
    }
  })

  const revokeAuthorityMutation = useMutation({
    mutationFn: () => api.delete<RevokeAuthorityResponse>(`/api/servers/${server.id}/agent-authority`),
    onSuccess: () => {
      projectServerCatalog(queryClient, {
        authority: {
          outstanding_offer: null,
          status: 'unclaimed'
        },
        kind: 'agent_authority_changed',
        serverId: server.id
      })
      setConfirmRevokeOpen(false)
      toast.success(t('agent_reenrollment.authority_revoked'))
    },
    onError: (err: unknown) => {
      const message =
        err instanceof ApiError || err instanceof Error ? err.message : t('agent_reenrollment.revoke_authority_failed')
      toast.error(message)
    }
  })

  const origin = typeof window !== 'undefined' ? window.location.origin : ''
  const capabilitySelection = resolveAgentCapabilitySelection(selectedCaps)
  const installCommand = issued
    ? `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --server-url '${origin}' --enrollment-code '${issued.enrollment.code}'${capabilitySelection.installArgument}`
    : ''

  const toggleCap = (key: string) => {
    setSelectedCaps((prev) => {
      const next = new Set(prev)
      if (next.has(key)) {
        next.delete(key)
      } else {
        next.add(key)
      }
      return next
    })
  }
  const resetCapsToDefault = () => setSelectedCaps(new Set(DEFAULT_AGENT_CAPABILITY_KEYS))
  const selectAllCaps = () => setSelectedCaps(new Set(ALL_AGENT_CAPABILITY_KEYS))
  const selectNoCaps = () => setSelectedCaps(new Set())

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('add_server.copied'))
    } catch {
      // Clipboard access denied; ignore.
    }
  }

  const reset = () => {
    setIssued(null)
    setEmergencyMode(true)
    setSelectedCaps(initialCapsFor(server.capabilities))
  }

  const handleClose = () => {
    onOpenChange(false)
  }

  const handleSubmit = (e?: FormEvent) => {
    e?.preventDefault()
    mutation.mutate({ mode: emergencyMode ? 'emergency' : 'graceful' })
  }

  const outstanding = server.agent_authority.outstanding_offer ?? null

  return (
    <>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t('agent_reenrollment.title')} · <span className="font-mono">{server.name}</span>
          </DialogTitle>
        </DialogHeader>

        {issued && (
          <>
            <DialogBody className="space-y-5">
              <p className="text-muted-foreground text-sm">{t('agent_reenrollment.description')}</p>

              <div className="space-y-4 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
                <p className="text-amber-600 text-sm dark:text-amber-500">{t('add_server.shown_once_warning')}</p>

                <div>
                  <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.code_label')}</p>
                  <div className="flex min-w-0 items-center gap-2">
                    <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                      {issued.enrollment.code}
                    </code>
                    <Button
                      aria-label={t('add_server.copy')}
                      onClick={() => copy(issued.enrollment.code)}
                      size="icon"
                      type="button"
                      variant="outline"
                    >
                      <Copy className="size-4" />
                    </Button>
                  </div>
                </div>

                <div>
                  <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.install_command')}</p>
                  <div className="flex min-w-0 items-start gap-2">
                    <code className="min-w-0 flex-1 break-all rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                      {installCommand}
                    </code>
                    <Button
                      aria-label={t('add_server.copy')}
                      onClick={() => copy(installCommand)}
                      size="icon"
                      type="button"
                      variant="outline"
                    >
                      <Copy className="size-4" />
                    </Button>
                  </div>
                </div>

                <div>
                  <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.steps_title')}</p>
                  <ol className="list-decimal space-y-1 pl-5 text-muted-foreground text-sm">
                    <li>{t('add_server.step1')}</li>
                    <li>{t('add_server.step2')}</li>
                    <li>{t('add_server.step3')}</li>
                  </ol>
                </div>
              </div>
            </DialogBody>

            <DialogFooter>
              <Button onClick={reset} type="button" variant="outline">
                {t('add_server.another')}
              </Button>
              <Button onClick={handleClose} type="button">
                {t('add_server.done')}
              </Button>
            </DialogFooter>
          </>
        )}
        {!issued && outstanding && (
          <OutstandingNotice
            authorityStatus={server.agent_authority.status}
            enrollment={outstanding}
            onClose={handleClose}
            onReplaced={setIssued}
            serverId={server.id}
          />
        )}
        {!(outstanding || issued) && (
          <form className="flex min-h-0 flex-1 flex-col gap-4" onSubmit={handleSubmit}>
            <DialogBody className="space-y-4">
              <p className="text-muted-foreground text-sm">{t('agent_reenrollment.description')}</p>

              <AgentCapabilityPicker
                hintKey="agent_reenrollment.caps_hint"
                idPrefix="agent-reenrollment-cap"
                labelKey="agent_reenrollment.caps_label"
                onReset={resetCapsToDefault}
                onSelectAll={selectAllCaps}
                onSelectNone={selectNoCaps}
                onToggle={toggleCap}
                selected={selectedCaps}
                t={t}
              />

              <fieldset className="space-y-2">
                <label
                  className="flex cursor-pointer items-center gap-2 text-sm"
                  htmlFor="agent-reenrollment-emergency"
                >
                  <Checkbox
                    checked={emergencyMode}
                    id="agent-reenrollment-emergency"
                    onCheckedChange={(checked) => setEmergencyMode(Boolean(checked))}
                  />
                  <span>{t('agent_reenrollment.emergency_mode')}</span>
                </label>
                {emergencyMode ? (
                  <p className="pl-6 text-amber-600 text-xs dark:text-amber-500">
                    {t('agent_reenrollment.emergency_description')}
                  </p>
                ) : (
                  <p className="pl-6 text-muted-foreground text-xs">{t('agent_reenrollment.graceful_description')}</p>
                )}
              </fieldset>

              <p className="text-muted-foreground text-xs">{t('agent_reenrollment.ttl_tip')}</p>
            </DialogBody>

            <DialogFooter>
              <Button onClick={handleClose} type="button" variant="outline">
                {t('common:cancel')}
              </Button>
              {server.agent_authority.status === 'claimed' && (
                <Button onClick={() => setConfirmRevokeOpen(true)} type="button" variant="destructive">
                  {t('agent_reenrollment.revoke_authority')}
                </Button>
              )}
              <Button
                className={cn(mutation.isPending && 'pointer-events-none opacity-70')}
                disabled={mutation.isPending}
                type="submit"
              >
                <RefreshCw aria-hidden="true" className="size-4" />
                {mutation.isPending ? t('agent_reenrollment.generating') : t('agent_reenrollment.generate')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>

      <AlertDialog onOpenChange={setConfirmRevokeOpen} open={confirmRevokeOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('agent_reenrollment.revoke_authority_title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('agent_reenrollment.revoke_authority_description', { name: server.name })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={revokeAuthorityMutation.isPending}>{t('common:cancel')}</AlertDialogCancel>
            <AlertDialogAction
              disabled={revokeAuthorityMutation.isPending}
              onClick={() => revokeAuthorityMutation.mutate()}
              variant="destructive"
            >
              {t('agent_reenrollment.revoke_authority')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
