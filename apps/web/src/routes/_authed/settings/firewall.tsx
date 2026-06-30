import { createFileRoute } from '@tanstack/react-router'
import { Plus, Search } from 'lucide-react'
import { useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { FirewallActivityLog } from '@/components/firewall/activity-log'
import { AddBlockDrawer, type AddBlockInitialValues } from '@/components/firewall/add-block-drawer'
import { BlockTable } from '@/components/firewall/block-table'
import { DeleteBlockDialog } from '@/components/firewall/delete-block-dialog'
import { FirewallKpiCards } from '@/components/firewall/kpi-cards'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { BlockListItem } from '@/types/firewall'

export const Route = createFileRoute('/_authed/settings/firewall')({
  component: FirewallPage
})

interface FirewallPageState {
  addInitial: AddBlockInitialValues | undefined
  addOpen: boolean
  deleteTarget: BlockListItem | null
  originFilter: string
  targetQuery: string
}

type FirewallPageAction =
  | { type: 'openAddBlock'; values?: AddBlockInitialValues }
  | { type: 'setAddOpen'; value: boolean }
  | { type: 'setDeleteTarget'; value: BlockListItem | null }
  | { type: 'setOriginFilter'; value: string }
  | { type: 'setTargetQuery'; value: string }

const INITIAL_FIREWALL_PAGE_STATE: FirewallPageState = {
  addInitial: undefined,
  addOpen: false,
  deleteTarget: null,
  originFilter: '',
  targetQuery: ''
}

function firewallPageReducer(state: FirewallPageState, action: FirewallPageAction): FirewallPageState {
  switch (action.type) {
    case 'openAddBlock':
      return { ...state, addInitial: action.values, addOpen: true }
    case 'setAddOpen':
      return { ...state, addOpen: action.value }
    case 'setDeleteTarget':
      return { ...state, deleteTarget: action.value }
    case 'setOriginFilter':
      return { ...state, originFilter: action.value }
    case 'setTargetQuery':
      return { ...state, targetQuery: action.value }
    default:
      return state
  }
}

function FirewallPage() {
  const { t } = useTranslation(['firewall', 'common'])
  const [state, dispatch] = useReducer(firewallPageReducer, INITIAL_FIREWALL_PAGE_STATE)

  const openAddBlock = (values?: AddBlockInitialValues) => {
    dispatch({ type: 'openAddBlock', values })
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <p className="mb-6 min-w-0 text-muted-foreground text-sm">
        {t('page.subtitle', { defaultValue: 'Block abusive IPs across one or more agents.' })}
      </p>

      <div className="mb-4">
        <FirewallKpiCards />
      </div>

      <Tabs defaultValue="blocklist">
        <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <TabsList>
            <TabsTrigger value="blocklist">{t('tabs.blocklist', { defaultValue: 'Blocklist' })}</TabsTrigger>
            <TabsTrigger value="activity">{t('tabs.activity', { defaultValue: 'Activity' })}</TabsTrigger>
          </TabsList>
          <Button onClick={() => openAddBlock()} size="sm">
            <Plus className="size-4" />
            {t('actions.add_block', { defaultValue: 'Block IP' })}
          </Button>
        </div>

        <TabsContent value="blocklist">
          <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-center">
            <div className="relative w-full min-w-0 max-w-sm flex-1">
              <Search
                aria-hidden="true"
                className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                className="pl-9"
                onChange={(e) => dispatch({ type: 'setTargetQuery', value: e.target.value })}
                placeholder={t('filter.target_search', { defaultValue: 'Search IP or CIDR' })}
                value={state.targetQuery}
              />
            </div>
            <Select
              items={{
                '': t('filter.origin_all', { defaultValue: 'All origins' }),
                manual: t('filter.origin_manual', { defaultValue: 'Manual' }),
                auto: t('filter.origin_auto', { defaultValue: 'Auto' })
              }}
              onValueChange={(value) => dispatch({ type: 'setOriginFilter', value: value ?? '' })}
              value={state.originFilter}
            >
              <SelectTrigger className="h-9 w-[180px]">
                <SelectValue placeholder={t('filter.origin', { defaultValue: 'All origins' })} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">{t('filter.origin_all', { defaultValue: 'All origins' })}</SelectItem>
                <SelectItem value="manual">{t('filter.origin_manual', { defaultValue: 'Manual' })}</SelectItem>
                <SelectItem value="auto">{t('filter.origin_auto', { defaultValue: 'Auto' })}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <BlockTable
            onDelete={(block) => dispatch({ type: 'setDeleteTarget', value: block })}
            originFilter={state.originFilter || null}
            targetQuery={state.targetQuery || null}
          />
        </TabsContent>

        <TabsContent value="activity">
          <FirewallActivityLog />
        </TabsContent>
      </Tabs>

      <AddBlockDrawer
        initialValues={state.addInitial}
        onOpenChange={(open) => dispatch({ type: 'setAddOpen', value: open })}
        open={state.addOpen}
      />

      <DeleteBlockDialog
        blockId={state.deleteTarget?.id ?? null}
        onOpenChange={(open) => {
          if (!open) {
            dispatch({ type: 'setDeleteTarget', value: null })
          }
        }}
        target={state.deleteTarget?.target ?? null}
      />
    </div>
  )
}
