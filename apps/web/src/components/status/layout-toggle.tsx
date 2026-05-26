import { LayoutGrid, List } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

interface Props {
  onChange: (next: 'list' | 'grid') => void
  value: 'list' | 'grid'
}

export function LayoutToggle({ value, onChange }: Props) {
  const { t } = useTranslation('status')
  return (
    <div className="inline-flex rounded-md border">
      <Button
        className={cn('rounded-r-none', value === 'grid' && 'bg-muted')}
        onClick={() => onChange('grid')}
        size="sm"
        title={t('layout_grid_tooltip')}
        variant="ghost"
      >
        <LayoutGrid className="size-4" />
      </Button>
      <Button
        className={cn('rounded-l-none border-l', value === 'list' && 'bg-muted')}
        onClick={() => onChange('list')}
        size="sm"
        title={t('layout_list_tooltip')}
        variant="ghost"
      >
        <List className="size-4" />
      </Button>
    </div>
  )
}
