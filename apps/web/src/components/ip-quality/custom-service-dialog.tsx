import { ArrowDownIcon, ArrowUpIcon, PlusIcon, Trash2Icon } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useCreateService, useUpdateService } from '@/hooks/use-ip-quality-api'
import type { UnlockMatch, UnlockRule, UnlockService, UnlockStatus } from '@/lib/ip-quality-types'

interface Props {
  onOpenChange: (open: boolean) => void
  open: boolean
  /** When set, the dialog edits this custom service; otherwise it creates one. */
  service?: UnlockService | null
}

type MatchKind = UnlockMatch['kind']

const CATEGORY_OPTIONS = ['streaming', 'ai', 'social', 'gaming', 'other']
const METHOD_OPTIONS = ['GET', 'HEAD', 'POST']
const STATUS_OPTIONS: UnlockStatus[] = ['unlocked', 'restricted', 'blocked', 'failed', 'unsupported']
const MATCH_KIND_OPTIONS: { value: MatchKind; label: string }[] = [
  { value: 'status_equals', label: 'Status equals' },
  { value: 'status_in_range', label: 'Status in range' },
  { value: 'body_regex', label: 'Body regex' },
  { value: 'redirect_matches', label: 'Redirect matches' }
]

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

interface RuleRowProps {
  canMoveDown: boolean
  canMoveUp: boolean
  index: number
  onChange: (rule: UnlockRule) => void
  onMove: (direction: -1 | 1) => void
  onRemove: () => void
  rule: UnlockRule
}

function RuleRow({ rule, index, canMoveUp, canMoveDown, onChange, onMove, onRemove }: RuleRowProps) {
  const { match } = rule

  const setKind = (kind: MatchKind) => onChange({ ...rule, match: defaultMatch(kind) })
  const setMatch = (next: UnlockMatch) => onChange({ ...rule, match: next })
  const setResult = (result: UnlockStatus) => onChange({ ...rule, result })

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3" data-testid="rule-row">
      <div className="flex items-center justify-between">
        <span className="font-medium text-muted-foreground text-xs">Rule {index + 1}</span>
        <div className="flex gap-1">
          <Button
            aria-label="Move rule up"
            disabled={!canMoveUp}
            onClick={() => onMove(-1)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <ArrowUpIcon />
          </Button>
          <Button
            aria-label="Move rule down"
            disabled={!canMoveDown}
            onClick={() => onMove(1)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <ArrowDownIcon />
          </Button>
          <Button aria-label="Remove rule" onClick={onRemove} size="icon-sm" type="button" variant="ghost">
            <Trash2Icon />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Select onValueChange={(v) => setKind(v as MatchKind)} value={match.kind}>
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {MATCH_KIND_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {match.kind === 'status_equals' && (
          <Input
            aria-label="Status code"
            className="w-24"
            onChange={(e) => setMatch({ kind: 'status_equals', code: Number(e.target.value) })}
            type="number"
            value={match.code}
          />
        )}
        {match.kind === 'status_in_range' && (
          <>
            <Input
              aria-label="Status range minimum"
              className="w-20"
              onChange={(e) => setMatch({ ...match, min: Number(e.target.value) })}
              type="number"
              value={match.min}
            />
            <Input
              aria-label="Status range maximum"
              className="w-20"
              onChange={(e) => setMatch({ ...match, max: Number(e.target.value) })}
              type="number"
              value={match.max}
            />
          </>
        )}
        {match.kind === 'body_regex' && (
          <Input
            aria-label="Body regex pattern"
            className="flex-1"
            onChange={(e) => setMatch({ kind: 'body_regex', pattern: e.target.value })}
            placeholder="regex"
            value={match.pattern}
          />
        )}
        {match.kind === 'redirect_matches' && (
          <Input
            aria-label="Redirect pattern"
            className="flex-1"
            onChange={(e) => setMatch({ kind: 'redirect_matches', pattern: e.target.value })}
            placeholder="redirect pattern"
            value={match.pattern}
          />
        )}
      </div>
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground text-xs">Result</span>
        <Select onValueChange={(v) => setResult(v as UnlockStatus)} value={rule.result}>
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {STATUS_OPTIONS.map((s) => (
              <SelectItem key={s} value={s}>
                {s}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  )
}

function parseExistingRules(service: UnlockService): RuleEntry[] {
  const fallback = (): RuleEntry[] => [{ uid: nextUid(), rule: defaultRule() }]
  if (!service.rules) {
    return fallback()
  }
  try {
    const parsed = JSON.parse(service.rules) as UnlockRule[]
    if (Array.isArray(parsed) && parsed.length > 0) {
      return parsed.map((rule) => ({ uid: nextUid(), rule }))
    }
    return fallback()
  } catch {
    return fallback()
  }
}

function parseExistingHeaders(service: UnlockService): HeaderRow[] {
  if (!service.request) {
    return []
  }
  try {
    const req = JSON.parse(service.request) as { headers?: [string, string][] }
    return (req.headers ?? []).map(([name, value]) => ({ uid: nextUid(), name, value }))
  } catch {
    return []
  }
}

function parseRequestField(service: UnlockService | null | undefined, field: 'url' | 'method'): string {
  if (!service?.request) {
    return field === 'method' ? 'GET' : ''
  }
  try {
    const req = JSON.parse(service.request) as { url?: string; method?: string }
    return req[field] ?? (field === 'method' ? 'GET' : '')
  } catch {
    return field === 'method' ? 'GET' : ''
  }
}

export function CustomServiceDialog({ open, onOpenChange, service }: Props) {
  const isEdit = Boolean(service)
  const createMutation = useCreateService()
  const updateMutation = useUpdateService()
  const isPending = createMutation.isPending || updateMutation.isPending

  const [name, setName] = useState('')
  const [category, setCategory] = useState('streaming')
  const [popularity, setPopularity] = useState(50)
  const [url, setUrl] = useState('')
  const [method, setMethod] = useState('GET')
  const [timeoutMs, setTimeoutMs] = useState(5000)
  const [headers, setHeaders] = useState<HeaderRow[]>([])
  const [rules, setRules] = useState<RuleEntry[]>([{ uid: nextUid(), rule: defaultRule() }])

  // Re-seed the form whenever the dialog opens or the edited service changes.
  useEffect(() => {
    if (!open) {
      return
    }
    if (service) {
      setName(service.name)
      setCategory(service.category)
      setPopularity(service.popularity)
      setUrl(parseRequestField(service, 'url'))
      setMethod(parseRequestField(service, 'method'))
      setHeaders(parseExistingHeaders(service))
      setRules(parseExistingRules(service))
      try {
        const req = JSON.parse(service.request ?? '{}') as { timeout_ms?: number }
        setTimeoutMs(req.timeout_ms ?? 5000)
      } catch {
        setTimeoutMs(5000)
      }
    } else {
      setName('')
      setCategory('streaming')
      setPopularity(50)
      setUrl('')
      setMethod('GET')
      setTimeoutMs(5000)
      setHeaders([])
      setRules([{ uid: nextUid(), rule: defaultRule() }])
    }
  }, [open, service])

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
      toast.success(isEdit ? 'Service updated' : 'Service created')
      onOpenChange(false)
    }
    const onError = (err: unknown) => {
      toast.error(err instanceof Error ? err.message : 'Request failed')
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
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isEdit ? 'Edit custom service' : 'New custom service'}</DialogTitle>
        </DialogHeader>
        <form className="flex min-h-0 flex-col" onSubmit={handleSubmit}>
          <DialogBody className="flex flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-name">Name</Label>
              <Input
                autoComplete="off"
                id="ipq-name"
                onChange={(e) => setName(e.target.value)}
                placeholder="My Service"
                required
                value={name}
              />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="ipq-category">Category</Label>
                <Select onValueChange={setCategory} value={category}>
                  <SelectTrigger className="w-full" id="ipq-category">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {CATEGORY_OPTIONS.map((c) => (
                      <SelectItem key={c} value={c}>
                        {c}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="ipq-popularity">Popularity</Label>
                <Input
                  id="ipq-popularity"
                  max={100}
                  min={0}
                  onChange={(e) => setPopularity(Number(e.target.value))}
                  type="number"
                  value={popularity}
                />
              </div>
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="ipq-url">URL</Label>
              <Input
                autoComplete="off"
                id="ipq-url"
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/check"
                required
                value={url}
              />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="ipq-method">Method</Label>
                <Select onValueChange={setMethod} value={method}>
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
                <Label htmlFor="ipq-timeout">Timeout (ms)</Label>
                <Input
                  id="ipq-timeout"
                  min={100}
                  onChange={(e) => setTimeoutMs(Number(e.target.value))}
                  type="number"
                  value={timeoutMs}
                />
              </div>
            </div>

            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <Label>Headers</Label>
                <Button
                  onClick={() => setHeaders((prev) => [...prev, { uid: nextUid(), name: '', value: '' }])}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  <PlusIcon />
                  Add header
                </Button>
              </div>
              {headers.map((header) => (
                <div className="flex items-center gap-2" data-testid="header-row" key={header.uid}>
                  <Input
                    aria-label="Header name"
                    onChange={(e) => updateHeader(header.uid, { name: e.target.value })}
                    placeholder="Header"
                    value={header.name}
                  />
                  <Input
                    aria-label="Header value"
                    onChange={(e) => updateHeader(header.uid, { value: e.target.value })}
                    placeholder="Value"
                    value={header.value}
                  />
                  <Button
                    aria-label="Remove header"
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
                <Label>Match rules (evaluated in order)</Label>
                <Button
                  onClick={() => setRules((prev) => [...prev, { uid: nextUid(), rule: defaultRule() }])}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  <PlusIcon />
                  Add rule
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
                />
              ))}
            </div>
          </DialogBody>
          <DialogFooter>
            <Button onClick={() => onOpenChange(false)} type="button" variant="outline">
              Cancel
            </Button>
            <Button disabled={isPending || name.trim().length === 0 || url.trim().length === 0} type="submit">
              {isEdit ? 'Save' : 'Create'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
