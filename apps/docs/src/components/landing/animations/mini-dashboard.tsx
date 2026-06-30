export function MiniDashboard() {
  return (
    <figure
      aria-label="Animated demo of a ServerBee server card"
      className="relative m-0 w-full max-w-md rounded-2xl border border-white/10 bg-white/[0.03] p-5 shadow-2xl shadow-amber-500/5 backdrop-blur"
    >
      <header className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="pulse-dot inline-block h-2.5 w-2.5 rounded-full bg-emerald-400" />
          <span className="font-medium text-sm text-zinc-100">edge-tokyo-01</span>
        </div>
        <span className="rounded-md bg-white/5 px-2 py-0.5 font-mono text-xs text-zinc-400">linux/arm64</span>
      </header>

      <div className="grid grid-cols-2 gap-4">
        <Ring color="#ffb300" label="CPU" value={42} />
        <Ring color="#4cc9f0" label="MEM" value={61} />
      </div>

      <div className="mt-5">
        <div className="mb-2 flex items-center justify-between text-xs text-zinc-400">
          <span>Network</span>
          <span className="font-mono text-zinc-300">↑ 2.1 MB/s · ↓ 318 KB/s</span>
        </div>
        <Sparkline />
      </div>

      <footer className="mt-4 grid grid-cols-3 gap-2 text-center text-xs">
        <Stat label="Load" value="0.42" />
        <Stat label="Disk" value="58%" />
        <Stat label="Uptime" value="14d" />
      </footer>
    </figure>
  )
}

function Ring({ label, value, color }: { label: string; value: number; color: string }) {
  const dash = 220
  return (
    <div className="flex items-center gap-3 rounded-xl bg-white/[0.03] p-3">
      <svg aria-hidden="true" focusable="false" height="60" viewBox="0 0 80 80" width="60">
        <circle cx="40" cy="40" fill="none" r="34" stroke="rgba(255,255,255,0.08)" strokeWidth="8" />
        <circle
          className="ring-anim"
          cx="40"
          cy="40"
          fill="none"
          r="34"
          stroke={color}
          strokeDasharray={dash}
          strokeLinecap="round"
          strokeWidth="8"
          transform="rotate(-90 40 40)"
        />
      </svg>
      <div>
        <div className="font-mono text-xs text-zinc-400">{label}</div>
        <div className="font-semibold text-xl text-zinc-100">{value}%</div>
      </div>
    </div>
  )
}

function Sparkline() {
  return (
    <div className="relative h-14 w-full overflow-hidden rounded-md bg-white/[0.03]">
      <svg
        aria-hidden="true"
        className="spark-scroll absolute inset-y-0 left-0 h-full w-[200%]"
        focusable="false"
        preserveAspectRatio="none"
        viewBox="0 0 400 56"
      >
        <defs>
          <linearGradient id="spark-fill" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#ffb300" stopOpacity="0.45" />
            <stop offset="100%" stopColor="#ffb300" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path
          d="M0 38 L20 30 L40 34 L60 22 L80 28 L100 18 L120 26 L140 14 L160 24 L180 16 L200 30 L220 22 L240 32 L260 18 L280 28 L300 20 L320 34 L340 24 L360 30 L380 22 L400 28 L400 56 L0 56 Z"
          fill="url(#spark-fill)"
        />
        <path
          d="M0 38 L20 30 L40 34 L60 22 L80 28 L100 18 L120 26 L140 14 L160 24 L180 16 L200 30 L220 22 L240 32 L260 18 L280 28 L300 20 L320 34 L340 24 L360 30 L380 22 L400 28"
          fill="none"
          stroke="#ffb300"
          strokeWidth="1.5"
        />
      </svg>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg bg-white/[0.03] py-2">
      <div className="text-[10px] text-zinc-500 uppercase tracking-wider">{label}</div>
      <div className="font-mono text-sm text-zinc-200">{value}</div>
    </div>
  )
}
