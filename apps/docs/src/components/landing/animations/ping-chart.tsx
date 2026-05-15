export function PingChartAnim() {
  return (
    <div aria-label="Animated demo of network latency monitoring" className="flex h-full flex-col gap-3" role="img">
      <div className="relative h-32 w-full overflow-hidden rounded-lg bg-black/30">
        <svg
          aria-hidden="true"
          className="spark-scroll absolute inset-0 h-full w-[200%]"
          focusable="false"
          preserveAspectRatio="none"
          viewBox="0 0 400 100"
        >
          <path
            d="M0 60 L25 50 L50 64 L75 38 L100 56 L125 30 L150 44 L175 24 L200 56 L225 42 L250 70 L275 32 L300 52 L325 26 L350 60 L375 38 L400 50"
            fill="none"
            stroke="#4cc9f0"
            strokeWidth="1.8"
          />
        </svg>
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-black/30 via-transparent to-black/30" />
      </div>
      <div className="grid grid-cols-6 gap-1.5">
        {Array.from({ length: 18 }).map((_, i) => (
          <span
            className="fade-cycle h-2 rounded-sm"
            // biome-ignore lint/suspicious/noArrayIndexKey: static-length decorative grid
            key={i}
            style={{
              animationDelay: `${(i % 6) * 0.25}s`,
              background: i % 7 === 5 ? '#f59e0b' : '#22c55e'
            }}
          />
        ))}
      </div>
    </div>
  )
}
