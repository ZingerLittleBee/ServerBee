import { FileCog, Layers, TerminalSquare } from 'lucide-react'
import type { ReactNode } from 'react'

export function OrbitIconsAnim() {
  return (
    <div
      aria-label="Animated demo of terminal, file manager, and Docker orbiting the agent"
      className="relative flex h-40 items-center justify-center"
      role="img"
    >
      <div className="relative h-32 w-32 rounded-full border border-white/10">
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="h-9 w-9 rounded-lg bg-amber-400/15 ring-1 ring-amber-400/40" />
        </div>
        <div className="orbit-anim absolute inset-0">
          <OrbitItem angle={0} icon={<TerminalSquare className="h-4 w-4" />} />
          <OrbitItem angle={120} icon={<FileCog className="h-4 w-4" />} />
          <OrbitItem angle={240} icon={<Layers className="h-4 w-4" />} />
        </div>
      </div>
    </div>
  )
}

function OrbitItem({ angle, icon }: { angle: number; icon: ReactNode }) {
  return (
    <div
      className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
      style={{ transform: `translate(-50%, -50%) rotate(${angle}deg) translate(60px) rotate(-${angle}deg)` }}
    >
      <div className="orbit-counter flex h-8 w-8 items-center justify-center rounded-md border border-white/10 bg-white/[0.05] text-amber-300">
        {icon}
      </div>
    </div>
  )
}
