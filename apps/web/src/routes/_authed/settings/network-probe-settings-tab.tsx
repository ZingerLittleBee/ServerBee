import { useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { getNetworkProbeTypeLabel, getNetworkTargetDisplayName } from '@/lib/network-i18n'
import type { NetworkProbeSetting, NetworkProbeTarget } from '@/lib/network-types'

interface SettingsDraft {
  defaultTargetIds: string[]
  packetCount: number
  probeInterval: number
}

type SettingsDraftAction =
  | { type: 'set-packet-count'; value: number }
  | { type: 'set-probe-interval'; value: number }
  | { targetId: string; type: 'toggle-default-target' }

function createSettingsDraft(setting: NetworkProbeSetting | undefined): SettingsDraft {
  return {
    defaultTargetIds: setting?.default_target_ids ?? [],
    packetCount: setting?.packet_count ?? 10,
    probeInterval: setting?.interval ?? 60
  }
}

function settingsDraftReducer(draft: SettingsDraft, action: SettingsDraftAction): SettingsDraft {
  switch (action.type) {
    case 'set-packet-count':
      return { ...draft, packetCount: action.value }
    case 'set-probe-interval':
      return { ...draft, probeInterval: action.value }
    case 'toggle-default-target':
      return {
        ...draft,
        defaultTargetIds: draft.defaultTargetIds.includes(action.targetId)
          ? draft.defaultTargetIds.filter((targetId) => targetId !== action.targetId)
          : [...draft.defaultTargetIds, action.targetId]
      }
    default:
      return draft
  }
}

export function NetworkProbeSettingsTab({
  onSubmit,
  setting,
  targets,
  updatePending
}: {
  onSubmit: (setting: NetworkProbeSetting) => void
  setting: NetworkProbeSetting | undefined
  targets: NetworkProbeTarget[]
  updatePending: boolean
}) {
  const { t, i18n } = useTranslation('network')
  const language = i18n.resolvedLanguage ?? i18n.language
  const [draft, dispatch] = useReducer(settingsDraftReducer, setting, createSettingsDraft)

  return (
    <ScrollArea className="h-full">
      <form
        className="max-w-xl space-y-6 pb-1"
        onSubmit={(event) => {
          event.preventDefault()
          onSubmit({
            default_target_ids: draft.defaultTargetIds,
            interval: draft.probeInterval,
            packet_count: draft.packetCount
          })
        }}
      >
        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="probe-interval">
            {t('probe_interval')}
          </label>
          <Input
            autoComplete="off"
            id="probe-interval"
            max={600}
            min={30}
            name="probe-interval"
            onChange={(event) =>
              dispatch({
                type: 'set-probe-interval',
                value: Number.parseInt(event.target.value, 10) || 60
              })
            }
            type="number"
            value={draft.probeInterval}
          />
          <p className="text-muted-foreground text-xs">{t('probe_interval_desc')}</p>
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="packet-count">
            {t('packet_count')}
          </label>
          <Input
            autoComplete="off"
            id="packet-count"
            max={20}
            min={5}
            name="packet-count"
            onChange={(event) =>
              dispatch({
                type: 'set-packet-count',
                value: Number.parseInt(event.target.value, 10) || 10
              })
            }
            type="number"
            value={draft.packetCount}
          />
          <p className="text-muted-foreground text-xs">{t('packet_count_desc')}</p>
        </div>

        <div className="space-y-2">
          <p className="font-medium text-sm">{t('default_targets')}</p>
          <p className="text-muted-foreground text-xs">{t('default_targets_desc')}</p>
          {targets.length > 0 ? (
            <ScrollArea className="h-72 rounded-md border p-3">
              <div className="space-y-1.5">
                {targets.map((target) => (
                  // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                  <label className="flex cursor-pointer items-center gap-2 text-sm" key={target.id}>
                    <Checkbox
                      checked={draft.defaultTargetIds.includes(target.id)}
                      onCheckedChange={() => dispatch({ targetId: target.id, type: 'toggle-default-target' })}
                    />
                    <span>{getNetworkTargetDisplayName(t, language, target)}</span>
                    <span className="text-muted-foreground text-xs">
                      ({getNetworkProbeTypeLabel(t, target.probe_type)})
                    </span>
                  </label>
                ))}
              </div>
            </ScrollArea>
          ) : (
            <p className="text-muted-foreground text-xs">{t('no_targets')}</p>
          )}
        </div>

        <Button disabled={updatePending} size="sm" type="submit">
          {t('save')}
        </Button>
      </form>
    </ScrollArea>
  )
}
