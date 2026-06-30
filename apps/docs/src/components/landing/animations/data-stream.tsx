export function DataStreamAnim() {
  return (
    <figure
      aria-label="Animated demo of real-time WebSocket streaming"
      className="relative m-0 flex h-40 items-center justify-between px-6"
    >
      <Endpoint color="#ffb300" label="Server" />
      <div className="relative mx-3 h-px flex-1 bg-gradient-to-r from-amber-400/30 via-cyan-300/30 to-amber-400/30">
        <Particle colorClass="bg-amber-300" delay="0s" />
        <Particle colorClass="bg-cyan-300" delay="0.6s" reverse />
        <Particle colorClass="bg-amber-300" delay="1.2s" />
        <Particle colorClass="bg-cyan-300" delay="1.8s" reverse />
      </div>
      <Endpoint color="#4cc9f0" label="Agent" />
    </figure>
  )
}

function Endpoint({ label, color }: { label: string; color: string }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div
        className="h-10 w-10 rounded-lg border border-white/10 bg-white/[0.04]"
        style={{ boxShadow: `0 0 24px -6px ${color}` }}
      />
      <span className="font-mono text-[10px] text-zinc-400 uppercase tracking-wider">{label}</span>
    </div>
  )
}

function Particle({ delay, colorClass, reverse }: { delay: string; colorClass: string; reverse?: boolean }) {
  return (
    <span
      aria-hidden
      className={`stream-particle absolute top-1/2 h-1.5 w-3 -translate-y-1/2 rounded-full ${colorClass}`}
      style={{ animationDelay: delay, animationDirection: reverse ? 'reverse' : 'normal' }}
    />
  )
}
