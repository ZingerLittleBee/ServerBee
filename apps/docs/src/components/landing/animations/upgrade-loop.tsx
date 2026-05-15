import { RotateCw } from 'lucide-react'

export function UpgradeLoopAnim() {
  return (
    <div
      aria-label="Animated demo of auto-upgrade"
      className="flex h-full flex-col items-center justify-center gap-3"
      role="img"
    >
      <div className="relative flex h-14 w-14 items-center justify-center rounded-full border border-amber-400/30 bg-amber-400/10">
        <RotateCw className="upgrade-spin h-7 w-7 text-amber-300" />
      </div>
      <div className="flex items-center gap-2 font-mono text-xs">
        <span className="rounded-md bg-white/[0.04] px-2 py-0.5 text-zinc-500 line-through">v0.2.9</span>
        <span className="text-amber-300">→</span>
        <span className="rounded-md bg-emerald-400/15 px-2 py-0.5 text-emerald-300">v0.3.0</span>
      </div>
    </div>
  )
}
