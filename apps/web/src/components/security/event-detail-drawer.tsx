import { ExternalLink } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { SecurityEventDto } from '@/lib/api-schema'
import { EventTypeBadge, SeverityBadge } from './severity-badge'

interface Props {
  event: SecurityEventDto | null
  onOpenChange: (open: boolean) => void
}

function formatTime(value: string | null | undefined): string {
  if (!value) {
    return '—'
  }
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) {
    return value
  }
  return d.toLocaleString()
}

export function SecurityEventDetailDrawer({ event, onOpenChange }: Props) {
  const { t } = useTranslation('security')

  return (
    <Sheet onOpenChange={onOpenChange} open={event !== null}>
      <SheetContent className="w-[90vw] max-w-[640px] sm:w-[640px]" side="right">
        <SheetHeader>
          <SheetTitle>{t('detail.title', { defaultValue: 'Security Event Detail' })}</SheetTitle>
          <SheetDescription>{event ? formatTime(event.created_at) : null}</SheetDescription>
        </SheetHeader>
        {event && (
          <ScrollArea className="min-h-0 flex-1">
            <div className="space-y-4 px-4 pb-6">
              <div className="flex flex-wrap items-center gap-2">
                <EventTypeBadge eventType={event.event_type} firstSeen={event.first_seen} />
                <SeverityBadge severity={event.severity} />
                {event.first_seen && (
                  <span className="text-muted-foreground text-xs">
                    {t('detail.first_seen', { defaultValue: 'First seen from this source' })}
                  </span>
                )}
              </div>

              <dl className="grid grid-cols-1 gap-2 text-sm sm:grid-cols-2">
                <Field label={t('detail.source_ip', { defaultValue: 'Source IP' })} value={event.source_ip} />
                <Field
                  label={t('detail.source_port', { defaultValue: 'Source Port' })}
                  value={event.source_port == null ? '—' : String(event.source_port)}
                />
                <Field label={t('detail.username', { defaultValue: 'Username' })} value={event.username ?? '—'} />
                <Field label={t('detail.detector', { defaultValue: 'Detector' })} value={event.detector_source} />
                <Field
                  label={t('detail.started_at', { defaultValue: 'Started' })}
                  value={formatTime(event.started_at)}
                />
                <Field label={t('detail.ended_at', { defaultValue: 'Ended' })} value={formatTime(event.ended_at)} />
              </dl>

              <a
                className="inline-flex items-center gap-1.5 text-primary text-sm hover:underline"
                href={`https://www.virustotal.com/gui/ip-address/${encodeURIComponent(event.source_ip)}`}
                rel="noopener"
                target="_blank"
              >
                <ExternalLink className="size-3.5" />
                {t('detail.virustotal_link', { defaultValue: 'Look up on VirusTotal' })}
              </a>

              <div className="space-y-2">
                <p className="font-medium text-sm">{t('detail.evidence', { defaultValue: 'Evidence' })}</p>
                <pre className="overflow-x-auto rounded-md bg-muted p-3 text-xs">
                  {JSON.stringify(event.evidence ?? {}, null, 2)}
                </pre>
              </div>
            </div>
          </ScrollArea>
        )}
      </SheetContent>
    </Sheet>
  )
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="space-y-0.5">
      <dt className="text-muted-foreground text-xs uppercase tracking-wide">{label}</dt>
      <dd className="break-all font-mono text-xs">{value}</dd>
    </div>
  )
}
