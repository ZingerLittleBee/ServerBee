import type { TFunction } from 'i18next'
import { Checkbox } from '@/components/ui/checkbox'
import { CAP_DEFAULT, CAPABILITIES, hasCap } from '@/lib/capabilities'
import { cn } from '@/lib/utils'

export const DEFAULT_AGENT_CAPABILITY_KEYS = CAPABILITIES.flatMap((capability) =>
  hasCap(CAP_DEFAULT, capability.bit) ? [capability.key] : []
)
export const ALL_AGENT_CAPABILITY_KEYS = CAPABILITIES.map((capability) => capability.key)

export function agentCapabilityKeysForMask(mask: number | null | undefined): Set<string> {
  return new Set(
    CAPABILITIES.flatMap((capability) => (hasCap(mask ?? CAP_DEFAULT, capability.bit) ? [capability.key] : []))
  )
}

export function resolveAgentCapabilitySelection(selected: ReadonlySet<string>): {
  installArgument: string
  isDefault: boolean
  keys: string[]
} {
  const keys = ALL_AGENT_CAPABILITY_KEYS.filter((key) => selected.has(key))
  const isDefault =
    keys.length === DEFAULT_AGENT_CAPABILITY_KEYS.length &&
    DEFAULT_AGENT_CAPABILITY_KEYS.every((key) => selected.has(key))
  let installArgument = ` --caps ${keys.join(',')}`
  if (isDefault) {
    installArgument = ''
  } else if (keys.length === 0) {
    installArgument = " --caps ''"
  }
  return { installArgument, isDefault, keys }
}

interface AgentCapabilityPickerProps {
  hintKey: string
  idPrefix: string
  labelKey: string
  onReset: () => void
  onSelectAll: () => void
  onSelectNone: () => void
  onToggle: (key: string) => void
  selected: Set<string>
  t: TFunction
}

export function AgentCapabilityPicker({
  hintKey,
  idPrefix,
  labelKey,
  onReset,
  onSelectAll,
  onSelectNone,
  onToggle,
  selected,
  t
}: AgentCapabilityPickerProps) {
  return (
    <fieldset className="space-y-2">
      <legend className="mb-1 flex w-full items-center justify-between gap-2">
        <span className="font-medium text-muted-foreground text-xs uppercase tracking-wider">{t(labelKey)}</span>
        <span className="flex gap-2 text-xs">
          <button className="text-muted-foreground hover:text-foreground" onClick={onReset} type="button">
            {t('add_server.caps_reset')}
          </button>
          <span className="text-muted-foreground/50">·</span>
          <button className="text-muted-foreground hover:text-foreground" onClick={onSelectAll} type="button">
            {t('add_server.caps_select_all')}
          </button>
          <span className="text-muted-foreground/50">·</span>
          <button className="text-muted-foreground hover:text-foreground" onClick={onSelectNone} type="button">
            {t('add_server.caps_select_none')}
          </button>
        </span>
      </legend>
      <p className="text-muted-foreground text-xs">{t(hintKey)}</p>
      <div className="mt-2 space-y-3 rounded-md border bg-muted/30 p-3">
        <CapabilityGroup
          caps={CAPABILITIES.filter((capability) => capability.risk !== 'high')}
          idPrefix={idPrefix}
          onToggle={onToggle}
          selected={selected}
          t={t}
          title={t('add_server.caps_low_risk')}
          tone="standard"
        />
        <CapabilityGroup
          caps={CAPABILITIES.filter((capability) => capability.risk === 'high')}
          idPrefix={idPrefix}
          onToggle={onToggle}
          selected={selected}
          t={t}
          title={t('add_server.caps_high_risk')}
          tone="high"
        />
      </div>
    </fieldset>
  )
}

function CapabilityGroup({
  caps,
  idPrefix,
  onToggle,
  selected,
  t,
  title,
  tone
}: {
  caps: readonly (typeof CAPABILITIES)[number][]
  idPrefix: string
  onToggle: (key: string) => void
  selected: Set<string>
  t: TFunction
  title: string
  tone: 'high' | 'standard'
}) {
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
        {caps.map((capability) => {
          const id = `${idPrefix}-${capability.key}`
          return (
            <label className="flex cursor-pointer items-center gap-2 text-sm" htmlFor={id} key={capability.key}>
              <Checkbox
                checked={selected.has(capability.key)}
                id={id}
                onCheckedChange={() => onToggle(capability.key)}
              />
              <span className="truncate">{t(capability.labelKey)}</span>
            </label>
          )
        })}
      </div>
    </div>
  )
}
