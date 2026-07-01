import { Checkbox } from '@/components/ui/checkbox'

export function StatusPageServerCheckboxItem({
  checked,
  name,
  onToggle
}: {
  checked: boolean
  name: string
  onToggle: () => void
}) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
    <label className="flex items-center gap-2 text-sm">
      <Checkbox checked={checked} onCheckedChange={onToggle} />
      {name}
    </label>
  )
}
