export function ColorRingAnim() {
  const stops = ['#ffb300', '#4cc9f0', '#22c55e', '#a855f7', '#ef4444', '#ffb300']
  const gradient = stops.map((c, i) => `${c} ${(i / (stops.length - 1)) * 360}deg`).join(', ')
  return (
    <div
      aria-label="Animated demo of theme customization"
      className="flex h-full items-center justify-center"
      role="img"
    >
      <div className="relative">
        <div className="orbit-anim h-28 w-28 rounded-full" style={{ background: `conic-gradient(${gradient})` }} />
        <div className="absolute inset-2 rounded-full bg-zinc-950" />
        <div className="fade-cycle absolute inset-4 rounded-full bg-amber-400 shadow-[0_0_30px_-6px_rgba(255,179,0,0.7)]" />
        <div className="absolute -bottom-1 left-1/2 -translate-x-1/2 rounded-full bg-zinc-900/80 px-2 py-0.5 font-mono text-[10px] text-amber-300">
          OKLCH
        </div>
      </div>
    </div>
  )
}
