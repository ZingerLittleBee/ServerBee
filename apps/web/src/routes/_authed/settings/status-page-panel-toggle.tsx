import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'

export function StatusPagePanelToggle({
  checked,
  description,
  id,
  label,
  onChange
}: {
  checked: boolean
  description: string
  id: string
  label: string
  onChange: (next: boolean) => void
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="space-y-0.5">
        <Label htmlFor={id}>{label}</Label>
        <p className="text-muted-foreground text-xs">{description}</p>
      </div>
      <Switch checked={checked} id={id} onCheckedChange={onChange} />
    </div>
  )
}
