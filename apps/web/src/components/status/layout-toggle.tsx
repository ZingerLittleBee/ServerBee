import { LayoutGrid, Table2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'

interface Props {
  onChange: (next: 'list' | 'grid') => void
  value: 'list' | 'grid'
}

export function LayoutToggle({ value, onChange }: Props) {
  const { t } = useTranslation('status')
  return (
    <ToggleGroup
      multiple={false}
      onValueChange={(next) => {
        if (next.length > 0) {
          onChange(next[0] as 'list' | 'grid')
        }
      }}
      size="default"
      value={[value]}
      variant="outline"
    >
      <ToggleGroupItem aria-label={t('layout_list_tooltip')} value="list">
        <Table2 className="size-4" />
      </ToggleGroupItem>
      <ToggleGroupItem aria-label={t('layout_grid_tooltip')} value="grid">
        <LayoutGrid className="size-4" />
      </ToggleGroupItem>
    </ToggleGroup>
  )
}
