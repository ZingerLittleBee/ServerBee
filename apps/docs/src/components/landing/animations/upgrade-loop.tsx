import { RotateCw } from 'lucide-react'

export function UpgradeLoopAnim() {
  return (
    <div
      aria-label="Animated demo of auto-upgrade"
      className="flex h-full flex-col items-center justify-center gap-3"
      role="img"
    >
      <RotateCw className="upgrade-spin h-9 w-9 text-amber-300" />
      <div className="flex items-center gap-2 font-mono text-xs">
        <span className="text-zinc-500 line-through">v0.2.9</span>
        <span className="text-amber-300">→</span>
        <span className="text-emerald-300">v0.3.0</span>
      </div>
    </div>
  )
}
