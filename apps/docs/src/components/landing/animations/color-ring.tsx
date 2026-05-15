export function ColorRingAnim() {
  const stops = ['#ffb300', '#4cc9f0', '#22c55e', '#a855f7', '#ef4444', '#ffb300']
  const gradient = stops.map((c, i) => `${c} ${(i / (stops.length - 1)) * 360}deg`).join(', ')
  return (
    <div
      aria-label="Animated demo of theme customization"
      className="flex h-full items-center justify-center"
      role="img"
    >
      <div className="orbit-anim h-24 w-24 rounded-full" style={{ background: `conic-gradient(${gradient})` }}>
        <div className="m-2 h-20 w-20 rounded-full bg-zinc-950">
          <div className="fade-cycle m-2 h-16 w-16 rounded-full bg-amber-400" />
        </div>
      </div>
    </div>
  )
}
