import { File, FolderOpen, FolderTree, Upload } from 'lucide-react'
import type { ComponentType } from 'react'

export function FileTreeAnim() {
  return (
    <div
      aria-label="Animated demo of the file manager"
      className="flex h-full flex-col gap-2 font-mono text-xs"
      role="img"
    >
      <div className="min-h-0 flex-1 space-y-0.5 overflow-hidden rounded-lg border border-white/10 bg-black/30 px-3 py-2 text-zinc-300">
        <Row Icon={FolderTree} label="/var/log" />
        <Row Icon={FolderOpen} indent label="nginx" />
        <Row Icon={File} indent2 label="access.log" muted="2.4 MB" />
        <Row Icon={File} indent2 label="error.log" muted="312 KB" />
      </div>
      <div className="flex shrink-0 items-center gap-2.5">
        <Upload aria-hidden className="h-3 w-3 shrink-0 text-amber-300" />
        <span className="shrink-0 text-[10px] text-zinc-400">access.log → uploading</span>
        <div className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-white/5">
          <span className="light-band absolute inset-y-0 left-0 w-1/2 bg-gradient-to-r from-amber-400 via-amber-300 to-amber-400" />
        </div>
        <span className="shrink-0 font-mono text-[10px] text-amber-300">64%</span>
      </div>
    </div>
  )
}

function Row({
  Icon,
  label,
  indent,
  indent2,
  muted
}: {
  Icon: ComponentType<{ className?: string }>
  label: string
  indent?: boolean
  indent2?: boolean
  muted?: string
}) {
  let pad = ''
  if (indent2) {
    pad = 'pl-6'
  } else if (indent) {
    pad = 'pl-3'
  }
  return (
    <div className={`flex items-center gap-2 ${pad}`}>
      <Icon className="h-3.5 w-3.5 text-amber-300" />
      <span className="text-zinc-300">{label}</span>
      {muted ? <span className="ml-auto text-[10px] text-zinc-500">{muted}</span> : null}
    </div>
  )
}
