import { FileCog, Layers, TerminalSquare } from 'lucide-react'
import type { ReactNode } from 'react'

const RING_RADIUS = 56

export function OrbitIconsAnim() {
  return (
    <div
      aria-label="Animated demo of terminal, file manager, and Docker orbiting the agent"
      className="relative flex h-40 items-center justify-center"
      role="img"
    >
      <div className="relative h-36 w-36">
        <svg aria-hidden="true" className="absolute inset-0 h-full w-full" focusable="false" viewBox="0 0 144 144">
          <defs>
            <radialGradient cx="50%" cy="50%" id="orbit-glow" r="50%">
              <stop offset="0%" stopColor="rgba(255,179,0,0.18)" />
              <stop offset="100%" stopColor="rgba(255,179,0,0)" />
            </radialGradient>
          </defs>
          <circle cx="72" cy="72" fill="url(#orbit-glow)" r={RING_RADIUS + 16} />
          <circle
            cx="72"
            cy="72"
            fill="none"
            r={RING_RADIUS}
            stroke="rgba(255,179,0,0.35)"
            strokeDasharray="2 6"
            strokeWidth="1"
          />
        </svg>

        <div className="absolute inset-0 flex items-center justify-center">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-amber-400/15 ring-1 ring-amber-400/50">
            <span className="h-1.5 w-1.5 rounded-full bg-amber-300 shadow-[0_0_8px_2px_rgba(255,179,0,0.7)]" />
          </div>
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
      className="absolute top-1/2 left-1/2"
      style={{ transform: `rotate(${angle}deg) translate(0, -${RING_RADIUS}px) rotate(-${angle}deg)` }}
    >
      <div className="orbit-counter -mt-4 -ml-4 flex h-8 w-8 items-center justify-center rounded-md border border-white/10 bg-white/[0.05] text-amber-300">
        {icon}
      </div>
    </div>
  )
}
