import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Copy, RefreshCw } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ApiError, api } from '@/lib/api-client'
import type { OutstandingEnrollmentSummary, RecoverRequest, RecoverResponse, ServerResponse } from '@/lib/api-schema'
import { CAP_DEFAULT, CAPABILITIES, hasCap } from '@/lib/capabilities'
import { cn } from '@/lib/utils'

const DEFAULT_CAP_KEYS = CAPABILITIES.filter((c) => hasCap(CAP_DEFAULT, c.bit)).map((c) => c.key)
const ALL_CAP_KEYS = CAPABILITIES.map((c) => c.key)

interface CapGroupProps {
  caps: readonly (typeof CAPABILITIES)[number][]
  onToggle: (key: string) => void
  selected: Set<string>
  t: (key: string) => string
  title: string
  tone: 'high' | 'standard'
}

function CapGroup({ caps, onToggle, selected, t, title, tone }: CapGroupProps) {
  return (
    <div>
      <p
        className={cn(
          'mb-1.5 font-medium text-[11px] uppercase tracking-wide',
          tone === 'high' ? 'text-amber-600 dark:text-amber-500' : 'text-muted-foreground'
        )}
      >
        {title}
      </p>
      <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
        {caps.map((cap) => {
          const id = `recover-agent-cap-${cap.key}`
          return (
            <label className="flex cursor-pointer items-center gap-2 text-sm" htmlFor={id} key={cap.key}>
              <Checkbox checked={selected.has(cap.key)} id={id} onCheckedChange={() => onToggle(cap.key)} />
              <span className="truncate">{t(cap.labelKey)}</span>
            </label>
          )
        })}
      </div>
    </div>
  )
}

function formatCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(remainingMs / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`
}

interface OutstandingNoticeProps {
  enrollment: OutstandingEnrollmentSummary
  onClose: () => void
  serverId: string
}

function OutstandingNotice({ enrollment, onClose, serverId: _serverId }: OutstandingNoticeProps) {
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
    mutationFn: () => api.delete<void>(`/api/agent/enrollments/${enrollment.id}`),
    onSuccess: () => {
      toast.success(t('recover_agent.revoked'))
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    },
    onError: (err: unknown) => {
      const message = err instanceof ApiError || err instanceof Error ? err.message : t('recover_agent.revoke_failed')
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

  return (
    <>
      <DialogBody className="space-y-4">
        <div className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
          <p className="font-medium text-amber-700 text-sm dark:text-amber-400">
            {t('recover_agent.outstanding_notice_title')}
          </p>
          <p className="font-mono text-amber-700 text-xs tabular-nums dark:text-amber-400">{enrollment.code_prefix}…</p>
          <p className="text-amber-700 text-xs tabular-nums dark:text-amber-400">{countdownLabel}</p>
          <p className="text-muted-foreground text-xs">{t('recover_agent.outstanding_notice_body')}</p>
        </div>
      </DialogBody>
      <DialogFooter>
        <Button onClick={onClose} type="button" variant="outline">
          {t('common:close', { defaultValue: 'Close' })}
        </Button>
        <Button
          disabled={revokeMutation.isPending}
          onClick={() => revokeMutation.mutate()}
          type="button"
          variant="destructive"
        >
          <RefreshCw aria-hidden="true" className="size-3.5" />
          {t('recover_agent.revoke')}
        </Button>
      </DialogFooter>
    </>
  )
}

interface RecoverAgentDialogProps {
  onOpenChange: (open: boolean) => void
  open: boolean
  server: Pick<ServerResponse, 'id' | 'name' | 'capabilities' | 'outstanding_enrollment'>
}

function initialCapsFor(caps: number | null | undefined): Set<string> {
  return new Set(CAPABILITIES.filter((c) => hasCap(caps ?? CAP_DEFAULT, c.bit)).map((c) => c.key))
}

export function RecoverAgentDialog({ open, onOpenChange, server }: RecoverAgentDialogProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()

  const [selectedCaps, setSelectedCaps] = useState<Set<string>>(() => initialCapsFor(server.capabilities))
  const [revokeImmediately, setRevokeImmediately] = useState(true)
  const [issued, setIssued] = useState<RecoverResponse | null>(null)

  // Reset internal state when the dialog closes so a fresh open never leaks the
  // previous issued code or caps selection. We intentionally re-project caps
  // only when the dialog closes, so server-side updates to capabilities while
  // the dialog is open don't blow away pending user toggles.
  const serverCaps = server.capabilities
  useEffect(() => {
    if (!open) {
      setIssued(null)
      setRevokeImmediately(true)
      setSelectedCaps(initialCapsFor(serverCaps))
    }
  }, [open, serverCaps])

  const mutation = useMutation({
    mutationFn: (body: RecoverRequest) => api.post<RecoverResponse>(`/api/servers/${server.id}/recover`, body),
    onSuccess: (data) => {
      setIssued(data)
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    },
    onError: (err: unknown) => {
      const message = err instanceof ApiError || err instanceof Error ? err.message : t('recover_agent.generate_failed')
      toast.error(message)
    }
  })

  const origin = typeof window !== 'undefined' ? window.location.origin : ''
  const orderedCapSelection = ALL_CAP_KEYS.filter((k) => selectedCaps.has(k))
  const capsIsDefault =
    orderedCapSelection.length === DEFAULT_CAP_KEYS.length && DEFAULT_CAP_KEYS.every((k) => selectedCaps.has(k))
  const capsArg = (() => {
    if (capsIsDefault) {
      return ''
    }
    if (orderedCapSelection.length === 0) {
      return " --caps ''"
    }
    return ` --caps ${orderedCapSelection.join(',')}`
  })()
  const installCommand = issued
    ? `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --server-url '${origin}' --enrollment-code '${issued.enrollment.code}'${capsArg}`
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
  const resetCapsToDefault = () => setSelectedCaps(new Set(DEFAULT_CAP_KEYS))
  const selectAllCaps = () => setSelectedCaps(new Set(ALL_CAP_KEYS))
  const selectNoCaps = () => setSelectedCaps(new Set())

  const highRiskCaps = CAPABILITIES.filter((c) => c.risk === 'high')
  const standardCaps = CAPABILITIES.filter((c) => c.risk !== 'high')

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
    setRevokeImmediately(true)
    setSelectedCaps(initialCapsFor(server.capabilities))
  }

  const handleClose = () => {
    reset()
    onOpenChange(false)
  }

  const handleSubmit = (e?: FormEvent) => {
    e?.preventDefault()
    mutation.mutate({ revoke_immediately: revokeImmediately })
  }

  const outstanding = server.outstanding_enrollment ?? null

  return (
    <Dialog
      onOpenChange={(next) => {
        if (next) {
          onOpenChange(true)
        } else {
          handleClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t('recover_agent.title')} · <span className="font-mono">{server.name}</span>
          </DialogTitle>
        </DialogHeader>

        {outstanding && <OutstandingNotice enrollment={outstanding} onClose={handleClose} serverId={server.id} />}
        {!outstanding && issued && (
          <>
            <DialogBody className="space-y-5">
              <p className="text-muted-foreground text-sm">{t('recover_agent.description')}</p>

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
        {!(outstanding || issued) && (
          <form className="flex min-h-0 flex-1 flex-col gap-4" onSubmit={handleSubmit}>
            <DialogBody className="space-y-4">
              <p className="text-muted-foreground text-sm">{t('recover_agent.description')}</p>

              <fieldset className="space-y-2">
                <legend className="mb-1 flex w-full items-center justify-between gap-2">
                  <span className="font-medium text-muted-foreground text-xs uppercase tracking-wider">
                    {t('recover_agent.caps_label')}
                  </span>
                  <span className="flex gap-2 text-xs">
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={resetCapsToDefault}
                      type="button"
                    >
                      {t('add_server.caps_reset')}
                    </button>
                    <span className="text-muted-foreground/50">·</span>
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={selectAllCaps}
                      type="button"
                    >
                      {t('add_server.caps_select_all')}
                    </button>
                    <span className="text-muted-foreground/50">·</span>
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={selectNoCaps}
                      type="button"
                    >
                      {t('add_server.caps_select_none')}
                    </button>
                  </span>
                </legend>
                <p className="text-muted-foreground text-xs">{t('recover_agent.caps_hint')}</p>
                <div className="mt-2 space-y-3 rounded-md border bg-muted/30 p-3">
                  <CapGroup
                    caps={standardCaps}
                    onToggle={toggleCap}
                    selected={selectedCaps}
                    t={t}
                    title={t('add_server.caps_low_risk')}
                    tone="standard"
                  />
                  <CapGroup
                    caps={highRiskCaps}
                    onToggle={toggleCap}
                    selected={selectedCaps}
                    t={t}
                    title={t('add_server.caps_high_risk')}
                    tone="high"
                  />
                </div>
              </fieldset>

              <fieldset className="space-y-2">
                <label
                  className="flex cursor-pointer items-center gap-2 text-sm"
                  htmlFor="recover-agent-revoke-immediately"
                >
                  <Checkbox
                    checked={revokeImmediately}
                    id="recover-agent-revoke-immediately"
                    onCheckedChange={(checked) => setRevokeImmediately(Boolean(checked))}
                  />
                  <span>{t('recover_agent.revoke_immediately')}</span>
                </label>
                {revokeImmediately && (
                  <p className="pl-6 text-amber-600 text-xs dark:text-amber-500">{t('recover_agent.revoke_warning')}</p>
                )}
              </fieldset>

              <p className="text-muted-foreground text-xs">{t('recover_agent.ttl_tip')}</p>
            </DialogBody>

            <DialogFooter>
              <Button onClick={handleClose} type="button" variant="outline">
                {t('common:cancel')}
              </Button>
              <Button
                className={cn(mutation.isPending && 'pointer-events-none opacity-70')}
                disabled={mutation.isPending}
                type="submit"
              >
                <RefreshCw aria-hidden="true" className="size-4" />
                {mutation.isPending ? t('recover_agent.generating') : t('recover_agent.generate')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
