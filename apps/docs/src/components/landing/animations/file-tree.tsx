import { File, FolderOpen, FolderTree } from 'lucide-react'
import type { ComponentType } from 'react'

export function FileTreeAnim() {
  return (
    <div aria-label="Animated demo of the file manager" className="space-y-2 font-mono text-xs" role="img">
      <div className="space-y-1 rounded-lg border border-white/10 bg-black/30 p-3 text-zinc-300">
        <Row Icon={FolderTree} label="/var/log" />
        <Row Icon={FolderOpen} indent label="  ⤷ nginx" />
        <Row Icon={File} indent label="     access.log" />
        <Row Icon={File} indent label="     error.log" />
      </div>
      <div className="relative h-2 w-full overflow-hidden rounded-full bg-white/5">
        <span className="light-band absolute inset-y-0 left-0 w-1/2 bg-gradient-to-r from-amber-400 to-amber-200" />
      </div>
    </div>
  )
}

function Row({
  Icon,
  label,
  indent
}: {
  Icon: ComponentType<{ className?: string }>
  label: string
  indent?: boolean
}) {
  return (
    <div className={`flex items-center gap-2 ${indent ? 'pl-3' : ''}`}>
      <Icon className="h-3.5 w-3.5 text-amber-300" />
      <span className="text-zinc-300">{label}</span>
    </div>
  )
}
