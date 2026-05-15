export function PingChartAnim() {
  const cells = 60
  return (
    <div aria-label="Animated demo of network latency monitoring" className="flex h-full flex-col gap-3" role="img">
      <div className="relative w-full flex-1 overflow-hidden rounded-lg bg-black/30">
        <svg
          aria-hidden="true"
          className="spark-scroll absolute inset-0 h-full w-[200%]"
          focusable="false"
          preserveAspectRatio="none"
          viewBox="0 0 400 100"
        >
          <defs>
            <linearGradient id="ping-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="#4cc9f0" stopOpacity="0.35" />
              <stop offset="100%" stopColor="#4cc9f0" stopOpacity="0" />
            </linearGradient>
          </defs>
          <path
            d="M0 60 L25 50 L50 64 L75 38 L100 56 L125 30 L150 44 L175 24 L200 56 L225 42 L250 70 L275 32 L300 52 L325 26 L350 60 L375 38 L400 50 L400 100 L0 100 Z"
            fill="url(#ping-fill)"
          />
          <path
            d="M0 60 L25 50 L50 64 L75 38 L100 56 L125 30 L150 44 L175 24 L200 56 L225 42 L250 70 L275 32 L300 52 L325 26 L350 60 L375 38 L400 50"
            fill="none"
            stroke="#4cc9f0"
            strokeWidth="1.8"
          />
        </svg>
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-black/30 via-transparent to-black/30" />
        <div className="absolute top-2 right-2 flex items-center gap-1.5 rounded-md bg-black/40 px-2 py-0.5 font-mono text-[10px] text-cyan-300">
          <span className="pulse-dot inline-block h-1.5 w-1.5 rounded-full bg-cyan-300" />
          24 ms
        </div>
      </div>
      <div className="flex items-center justify-between">
        <span className="font-mono text-[10px] text-zinc-500 uppercase tracking-wider">last 60 probes</span>
        <span className="flex items-center gap-3 font-mono text-[10px] text-zinc-500">
          <span className="flex items-center gap-1">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" /> ok
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-400" /> loss
          </span>
        </span>
      </div>
      <div className="flex h-2.5 gap-[2px]">
        {Array.from({ length: cells }).map((_, i) => (
          <span
            className="fade-cycle h-full flex-1 rounded-[1px]"
            // biome-ignore lint/suspicious/noArrayIndexKey: static-length decorative grid
            key={i}
            style={{
              animationDelay: `${(i % 12) * 0.08}s`,
              background: i % 13 === 7 || i % 17 === 4 ? '#f59e0b' : '#22c55e'
            }}
          />
        ))}
      </div>
    </div>
  )
}
