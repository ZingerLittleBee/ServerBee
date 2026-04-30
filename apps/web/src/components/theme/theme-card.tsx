import { Check, Copy, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

interface ThemeCardActions {
  onDelete?: () => void
  onDuplicate?: () => void
  onEdit?: () => void
}

interface ThemeCardProps {
  actions?: ThemeCardActions
  active: boolean
  name: string
  onActivate: () => void
  preview: string[]
}

export function ThemeCard({ actions, active, name, onActivate, preview }: ThemeCardProps) {
  const { t } = useTranslation('settings')
  const hasActions = actions !== undefined
  const swatches =
    preview.length > 0 ? preview : ['var(--primary)', 'var(--accent)', 'var(--background)', 'var(--foreground)']

  return (
    <div className="group relative">
      <button
        className={`w-full rounded-lg border-2 p-3 text-left transition-all hover:shadow-md ${
          active ? 'border-primary shadow-sm' : 'border-border hover:border-primary/50'
        }`}
        onClick={onActivate}
        type="button"
      >
        <div className="mb-2 flex gap-1.5">
          {swatches.map((color) => (
            <div
              className="size-6 rounded-full border border-black/10"
              key={`${name}-${color}`}
              style={{ backgroundColor: color }}
            />
          ))}
        </div>
        <div className="flex items-center gap-1.5">
          <span className="font-medium text-sm">{name}</span>
          {active && <Check className="size-3.5 text-primary" />}
        </div>
      </button>

      {hasActions && (
        <div className="absolute top-2 right-2 hidden gap-1 group-focus-within:flex group-hover:flex">
          {actions.onEdit && (
            <Button
              aria-label={t('appearance.custom_themes.edit')}
              onClick={(event) => {
                event.stopPropagation()
                actions.onEdit?.()
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Pencil className="size-3.5" />
            </Button>
          )}
          {actions.onDuplicate && (
            <Button
              aria-label={t('appearance.custom_themes.duplicate')}
              onClick={(event) => {
                event.stopPropagation()
                actions.onDuplicate?.()
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Copy className="size-3.5" />
            </Button>
          )}
          {actions.onDelete && (
            <Button
              aria-label={t('appearance.custom_themes.delete')}
              onClick={(event) => {
                event.stopPropagation()
                actions.onDelete?.()
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Trash2 className="size-3.5" />
            </Button>
          )}
        </div>
      )}
    </div>
  )
}
