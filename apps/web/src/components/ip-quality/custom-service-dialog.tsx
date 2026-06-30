import type { TFunction } from 'i18next'
import { ArrowDownIcon, ArrowUpIcon, PlusIcon, Trash2Icon } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useCreateService, useUpdateService } from '@/hooks/use-ip-quality-api'
import { CATEGORY_ORDER, categoryLabel } from '@/lib/ip-quality-constants'
import type { UnlockMatch, UnlockRule, UnlockService, UnlockStatus } from '@/lib/ip-quality-types'

interface Props {
  onOpenChange: (open: boolean) => void
  open: boolean
  /** When set, the dialog edits this custom service; otherwise it creates one. */
  service?: UnlockService | null
}

type MatchKind = UnlockMatch['kind']

const METHOD_OPTIONS = ['GET', 'HEAD', 'POST']
const STATUS_OPTIONS: UnlockStatus[] = ['unlocked', 'restricted', 'blocked', 'failed', 'unsupported']
const MATCH_KIND_OPTIONS: MatchKind[] = ['status_equals', 'status_in_range', 'body_regex', 'redirect_matches']

// Monotonic id generator so header/rule rows keep a stable React key across
// reordering and removal.
let uidCounter = 0
function nextUid(): string {
  uidCounter += 1
  return `row-${uidCounter}`
}

interface HeaderRow {
  name: string
  uid: string
  value: string
}

interface RuleEntry {
  rule: UnlockRule
  uid: string
}

function defaultMatch(kind: MatchKind): UnlockMatch {
  switch (kind) {
    case 'status_equals':
      return { kind, code: 200 }
    case 'status_in_range':
      return { kind, min: 200, max: 299 }
    case 'body_regex':
      return { kind, pattern: '' }
    case 'redirect_matches':
      return { kind, pattern: '' }
    default:
      return { kind: 'status_equals', code: 200 }
  }
}

function defaultRule(): UnlockRule {
  return { match: { kind: 'status_equals', code: 200 }, result: 'unlocked' }
}

const DEFAULT_TIMEOUT_MS = 5000

// Coerce a number-input value to a valid number. `Number(...)` yields `NaN`
// for a partially-typed `-` / `e`, and `NaN` serializes to `null`, which would
// silently corrupt the value — fall back to 0 instead.
export function toNumber(value: string): number {
  const parsed = Number.parseInt(value, 10)
  return Number.isNaN(parsed) ? 0 : parsed
}

interface RuleRowProps {
  canMoveDown: boolean
  canMoveUp: boolean
  index: number
  onChange: (rule: UnlockRule) => void
  onMove: (direction: -1 | 1) => void
  onRemove: () => void
  rule: UnlockRule
  t: TFunction
}

function RuleRow({ rule, index, canMoveUp, canMoveDown, onChange, onMove, onRemove, t }: RuleRowProps) {
  const { match } = rule

  const setKind = (kind: MatchKind) => onChange({ ...rule, match: defaultMatch(kind) })
  const setMatch = (next: UnlockMatch) => onChange({ ...rule, match: next })
  const setResult = (result: UnlockStatus) => onChange({ ...rule, result })

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3" data-testid="rule-row">
      <div className="flex items-center justify-between">
        <span className="font-medium text-muted-foreground text-xs">{t('dialog_rule_label', { n: index + 1 })}</span>
        <div className="flex gap-1">
          <Button
            aria-label={t('dialog_move_rule_up')}
            disabled={!canMoveUp}
            onClick={() => onMove(-1)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <ArrowUpIcon />
          </Button>
          <Button
            aria-label={t('dialog_move_rule_down')}
            disabled={!canMoveDown}
            onClick={() => onMove(1)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <ArrowDownIcon />
          </Button>
          <Button aria-label={t('dialog_remove_rule')} onClick={onRemove} size="icon-sm" type="button" variant="ghost">
            <Trash2Icon />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Select
          items={Object.fromEntries(MATCH_KIND_OPTIONS.map((kind) => [kind, t(`dialog_match_${kind}`)]))}
          onValueChange={(v) => setKind(v as MatchKind)}
          value={match.kind}
        >
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {MATCH_KIND_OPTIONS.map((kind) => (
              <SelectItem key={kind} value={kind}>
                {t(`dialog_match_${kind}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {match.kind === 'status_equals' && (
          <Input
            aria-label={t('dialog_status_code_aria')}
            className="w-24"
            onChange={(e) => setMatch({ kind: 'status_equals', code: toNumber(e.target.value) })}
            type="number"
            value={match.code}
          />
        )}
        {match.kind === 'status_in_range' && (
          <>
            <Input
              aria-label={t('dialog_status_min_aria')}
              className="w-20"
              onChange={(e) => setMatch({ ...match, min: toNumber(e.target.value) })}
              type="number"
              value={match.min}
            />
            <Input
              aria-label={t('dialog_status_max_aria')}
              className="w-20"
              onChange={(e) => setMatch({ ...match, max: toNumber(e.target.value) })}
              type="number"
              value={match.max}
            />
          </>
        )}
        {match.kind === 'body_regex' && (
          <Input
            aria-label={t('dialog_body_regex_aria')}
            className="flex-1"
            onChange={(e) => setMatch({ kind: 'body_regex', pattern: e.target.value })}
            placeholder={t('dialog_regex_placeholder')}
            value={match.pattern}
          />
        )}
        {match.kind === 'redirect_matches' && (
          <Input
            aria-label={t('dialog_redirect_aria')}
            className="flex-1"
            onChange={(e) => setMatch({ kind: 'redirect_matches', pattern: e.target.value })}
            placeholder={t('dialog_redirect_placeholder')}
            value={match.pattern}
          />
        )}
      </div>
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground text-xs">{t('dialog_result')}</span>
        <Select
          items={Object.fromEntries(STATUS_OPTIONS.map((s) => [s, t(`status_${s}`)]))}
          onValueChange={(v) => setResult(v as UnlockStatus)}
          value={rule.result}
        >
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {STATUS_OPTIONS.map((s) => (
              <SelectItem key={s} value={s}>
                {t(`status_${s}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  )
}

/** A custom service's request config, parsed from the stored JSON string. */
export interface ParsedRequest {
  headers: [string, string][]
  method: string
  timeout_ms: number
  url: string
}

/**
 * Parse a service's `request` JSON string in a single pass.
 *
 * Intentional asymmetry vs `parseExistingRules`: headers legitimately default
 * to `[]` (a custom service may send no extra headers), whereas rules always
 * default to ≥1 (a service with zero match rules can never classify a result).
 */
export function parseRequest(service: UnlockService | null | undefined): ParsedRequest {
  const fallback: ParsedRequest = { url: '', method: 'GET', timeout_ms: DEFAULT_TIMEOUT_MS, headers: [] }
  if (!service?.request) {
    return fallback
  }
  try {
    const req = JSON.parse(service.request) as {
      url?: string
      method?: string
      timeout_ms?: number
      headers?: [string, string][]
    }
    return {
      url: req.url ?? '',
      method: req.method ?? 'GET',
      timeout_ms: typeof req.timeout_ms === 'number' ? req.timeout_ms : DEFAULT_TIMEOUT_MS,
      headers: Array.isArray(req.headers) ? req.headers : []
    }
  } catch {
    return fallback
  }
}

/**
 * Parse a service's `rules` JSON string.
 *
 * Unlike `parseRequest`'s headers (which default to `[]`), rules always fall
 * back to a single default rule — a custom service must have at least one match
 * rule to ever produce a non-failed result.
 */
export function parseExistingRules(service: UnlockService | null | undefined): UnlockRule[] {
  if (!service?.rules) {
    return [defaultRule()]
  }
  try {
    const parsed = JSON.parse(service.rules) as UnlockRule[]
    return Array.isArray(parsed) && parsed.length > 0 ? parsed : [defaultRule()]
  } catch {
    return [defaultRule()]
  }
}

export function CustomServiceDialog({ open, onOpenChange, service }: Props) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      {open && (
        <CustomServiceDialogContent
          key={service?.id ?? 'new-custom-service'}
          onOpenChange={onOpenChange}
          service={service}
        />
      )}
    </Dialog>
  )
}

function CustomServiceDialogContent({
  onOpenChange,
  service
}: {
  onOpenChange: (open: boolean) => void
  service?: UnlockService | null
}) {
  const { t } = useTranslation('ip-quality')
  const isEdit = Boolean(service)
  const createMutation = useCreateService()
  const updateMutation = useUpdateService()
  const isPending = createMutation.isPending || updateMutation.isPending
  const request = parseRequest(service)

  const [name, setName] = useState(service?.name ?? '')
  const [category, setCategory] = useState(service?.category ?? 'streaming')
  const [popularity, setPopularity] = useState(service?.popularity ?? 50)
  const [url, setUrl] = useState(request.url)
  const [method, setMethod] = useState(request.method)
  const [timeoutMs, setTimeoutMs] = useState(request.timeout_ms)
  const [headers, setHeaders] = useState<HeaderRow[]>(() =>
    request.headers.map(([headerName, value]) => ({ uid: nextUid(), name: headerName, value }))
  )
  const [rules, setRules] = useState<RuleEntry[]>(() =>
    parseExistingRules(service).map((rule) => ({ uid: nextUid(), rule }))
  )

  const updateRule = (uid: string, rule: UnlockRule) => {
    setRules((prev) => prev.map((entry) => (entry.uid === uid ? { ...entry, rule } : entry)))
  }

  const moveRule = (uid: string, direction: -1 | 1) => {
    setRules((prev) => {
      const index = prev.findIndex((entry) => entry.uid === uid)
      const target = index + direction
      if (index === -1 || target < 0 || target >= prev.length) {
        return prev
      }
      const next = [...prev]
      ;[next[index], next[target]] = [next[target], next[index]]
      return next
    })
  }

  const removeRule = (uid: string) => {
    setRules((prev) => (prev.length <= 1 ? prev : prev.filter((entry) => entry.uid !== uid)))
  }

  const updateHeader = (uid: string, patch: Partial<HeaderRow>) => {
    setHeaders((prev) => prev.map((h) => (h.uid === uid ? { ...h, ...patch } : h)))
  }

  const removeHeader = (uid: string) => {
    setHeaders((prev) => prev.filter((h) => h.uid !== uid))
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0 || url.trim().length === 0) {
      return
    }

    const headerPairs: [string, string][] = headers
      .filter((h) => h.name.trim().length > 0)
      .map((h) => [h.name.trim(), h.value])
    const rulePayload: UnlockRule[] = rules.map((entry) => entry.rule)

    const onSuccess = () => {
      toast.success(isEdit ? t('dialog_updated') : t('dialog_created'))
      onOpenChange(false)
    }
    const onError = (err: unknown) => {
      toast.error(err instanceof Error ? err.message : t('dialog_request_failed'))
    }

    if (isEdit && service) {
      updateMutation.mutate(
        {
          id: service.id,
          name: name.trim(),
          category,
          popularity,
          url: url.trim(),
          method,
          headers: headerPairs,
          timeout_ms: timeoutMs,
          rules: rulePayload
        },
        { onSuccess, onError }
      )
    } else {
      createMutation.mutate(
        {
          name: name.trim(),
          category,
          popularity,
          url: url.trim(),
          method,
          headers: headerPairs,
          timeout_ms: timeoutMs,
          rules: rulePayload
        },
        { onSuccess, onError }
      )
    }
  }

  return (
    <DialogContent className="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>{isEdit ? t('dialog_edit_title') : t('dialog_create_title')}</DialogTitle>
      </DialogHeader>
      <form className="flex min-h-0 flex-col" onSubmit={handleSubmit}>
        <DialogBody className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ipq-name">{t('dialog_name')}</Label>
            <Input
              autoComplete="off"
              id="ipq-name"
              onChange={(e) => setName(e.target.value)}
              placeholder={t('dialog_name_placeholder')}
              required
              value={name}
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-category">{t('dialog_category')}</Label>
              <Select
                items={Object.fromEntries(CATEGORY_ORDER.map((c) => [c, categoryLabel(c)]))}
                onValueChange={(v) => setCategory(v ?? 'streaming')}
                value={category}
              >
                <SelectTrigger className="w-full" id="ipq-category">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CATEGORY_ORDER.map((c) => (
                    <SelectItem key={c} value={c}>
                      {categoryLabel(c)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-popularity">{t('dialog_popularity')}</Label>
              <Input
                id="ipq-popularity"
                max={100}
                min={0}
                onChange={(e) => setPopularity(toNumber(e.target.value))}
                type="number"
                value={popularity}
              />
            </div>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ipq-url">{t('dialog_url')}</Label>
            <Input
              autoComplete="off"
              id="ipq-url"
              onChange={(e) => setUrl(e.target.value)}
              placeholder={t('dialog_url_placeholder')}
              required
              value={url}
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-method">{t('dialog_method')}</Label>
              <Select onValueChange={(v) => setMethod(v ?? 'GET')} value={method}>
                <SelectTrigger className="w-full" id="ipq-method">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {METHOD_OPTIONS.map((m) => (
                    <SelectItem key={m} value={m}>
                      {m}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-timeout">{t('dialog_timeout')}</Label>
              <Input
                id="ipq-timeout"
                min={100}
                onChange={(e) => setTimeoutMs(toNumber(e.target.value))}
                type="number"
                value={timeoutMs}
              />
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <Label>{t('dialog_headers')}</Label>
              <Button
                onClick={() => setHeaders((prev) => [...prev, { uid: nextUid(), name: '', value: '' }])}
                size="sm"
                type="button"
                variant="outline"
              >
                <PlusIcon />
                {t('dialog_add_header')}
              </Button>
            </div>
            {headers.map((header) => (
              <div className="flex items-center gap-2" data-testid="header-row" key={header.uid}>
                <Input
                  aria-label={t('dialog_header_name_aria')}
                  onChange={(e) => updateHeader(header.uid, { name: e.target.value })}
                  placeholder={t('dialog_header_name_placeholder')}
                  value={header.name}
                />
                <Input
                  aria-label={t('dialog_header_value_aria')}
                  onChange={(e) => updateHeader(header.uid, { value: e.target.value })}
                  placeholder={t('dialog_header_value_placeholder')}
                  value={header.value}
                />
                <Button
                  aria-label={t('dialog_remove_header')}
                  onClick={() => removeHeader(header.uid)}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <Trash2Icon />
                </Button>
              </div>
            ))}
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <Label>{t('dialog_rules')}</Label>
              <Button
                onClick={() => setRules((prev) => [...prev, { uid: nextUid(), rule: defaultRule() }])}
                size="sm"
                type="button"
                variant="outline"
              >
                <PlusIcon />
                {t('dialog_add_rule')}
              </Button>
            </div>
            {rules.map((entry, index) => (
              <RuleRow
                canMoveDown={index < rules.length - 1}
                canMoveUp={index > 0}
                index={index}
                key={entry.uid}
                onChange={(r) => updateRule(entry.uid, r)}
                onMove={(dir) => moveRule(entry.uid, dir)}
                onRemove={() => removeRule(entry.uid)}
                rule={entry.rule}
                t={t}
              />
            ))}
          </div>
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} type="button" variant="outline">
            {t('dialog_cancel')}
          </Button>
          <Button disabled={isPending || name.trim().length === 0 || url.trim().length === 0} type="submit">
            {isEdit ? t('dialog_save') : t('dialog_create')}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
