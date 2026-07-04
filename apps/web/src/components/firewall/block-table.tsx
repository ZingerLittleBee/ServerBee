import type { TFunction } from 'i18next'
import { Trash2 } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useFirewallBlocks } from '@/hooks/use-firewall-blocks'
import { formatDateTime } from '@/lib/format'
import type { BlockListItem } from '@/types/firewall'

interface Props {
  onDelete: (block: BlockListItem) => void
  originFilter?: string | null
  targetQuery?: string | null
}

function formatTime(value: string | null | undefined): string {
  if (!value) {
    return '—'
  }
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) {
    return value
  }
  return formatDateTime(d)
}

function scopeLabel(t: TFunction, coverType: BlockListItem['cover_type']): string {
  if (coverType === 'include') {
    return t('add.scope_include', { defaultValue: 'Selected servers' })
  }
  if (coverType === 'exclude') {
    return t('add.scope_exclude', { defaultValue: 'All except selected' })
  }
  return t('add.scope_all', { defaultValue: 'All servers' })
}

function originLabel(t: TFunction, origin: BlockListItem['origin']): string {
  return origin === 'auto'
    ? t('filter.origin_auto', { defaultValue: 'Auto' })
    : t('filter.origin_manual', { defaultValue: 'Manual' })
}

export function BlockTable({ onDelete, originFilter, targetQuery }: Props) {
  const { t } = useTranslation(['firewall', 'common'])
  const query = useFirewallBlocks({
    origin: originFilter ?? null,
    target_q: targetQuery ?? null,
    limit: 50
  })

  const rows = useMemo(() => {
    const out: BlockListItem[] = []
    for (const page of query.data?.pages ?? []) {
      for (const item of page.items) {
        out.push(item)
      }
    }
    return out
  }, [query.data])

  return (
    <div className="rounded-md border">
      <ScrollArea className="max-h-[560px]">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('table.target', { defaultValue: 'Target' })}</TableHead>
              <TableHead className="w-[80px]">{t('table.family', { defaultValue: 'Family' })}</TableHead>
              <TableHead className="w-[120px]">{t('table.scope', { defaultValue: 'Scope' })}</TableHead>
              <TableHead className="w-[100px]">{t('table.origin', { defaultValue: 'Origin' })}</TableHead>
              <TableHead>{t('table.comment', { defaultValue: 'Comment' })}</TableHead>
              <TableHead className="w-[180px]">{t('table.created', { defaultValue: 'Created' })}</TableHead>
              <TableHead className="w-[80px] text-right">{t('table.actions', { defaultValue: 'Actions' })}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {query.isLoading &&
              Array.from({ length: 5 }, (_, i) => (
                <TableRow key={`fw-skel-${i.toString()}`}>
                  <TableCell colSpan={7}>
                    <Skeleton className="h-6 w-full" />
                  </TableCell>
                </TableRow>
              ))}
            {!query.isLoading && rows.length === 0 && (
              <TableRow>
                <TableCell className="text-center text-muted-foreground" colSpan={7}>
                  {t('table.empty', { defaultValue: 'No firewall blocks yet.' })}
                </TableCell>
              </TableRow>
            )}
            {!query.isLoading &&
              rows.map((block) => (
                <TableRow data-testid="firewall-block-row" key={block.id}>
                  <TableCell className="font-mono text-xs">{block.target}</TableCell>
                  <TableCell className="font-mono text-xs">v{block.family}</TableCell>
                  <TableCell className="text-xs">{scopeLabel(t, block.cover_type)}</TableCell>
                  <TableCell>
                    <span
                      className={`rounded px-1.5 py-0.5 text-xs ${
                        block.origin === 'auto'
                          ? 'bg-orange-100 text-orange-800 dark:bg-orange-950 dark:text-orange-200'
                          : 'bg-muted text-muted-foreground'
                      }`}
                    >
                      {originLabel(t, block.origin)}
                    </span>
                  </TableCell>
                  <TableCell className="max-w-xs truncate text-muted-foreground text-xs">
                    {block.comment ?? '—'}
                  </TableCell>
                  <TableCell className="whitespace-nowrap font-mono text-muted-foreground text-xs">
                    {formatTime(block.created_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      aria-label={t('common:delete')}
                      onClick={() => onDelete(block)}
                      size="icon-sm"
                      variant="ghost"
                    >
                      <Trash2 aria-hidden="true" className="size-3.5" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
          </TableBody>
        </Table>
      </ScrollArea>
      {query.hasNextPage && (
        <div className="flex justify-center border-t p-2">
          <Button disabled={query.isFetchingNextPage} onClick={() => query.fetchNextPage()} size="sm" variant="outline">
            {query.isFetchingNextPage
              ? t('table.loading_more', { defaultValue: 'Loading…' })
              : t('table.load_more', { defaultValue: 'Load more' })}
          </Button>
        </div>
      )}
    </div>
  )
}
