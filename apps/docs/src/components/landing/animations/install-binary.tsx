export function InstallBinaryAnim() {
  return (
    <figure
      aria-label="Animated demo of installing the ServerBee binary"
      className="m-0 flex h-40 items-center justify-center"
    >
      <div className="relative flex flex-col items-center gap-3">
        <div className="rounded-lg border border-amber-400/40 bg-amber-400/10 px-4 py-2 font-mono text-amber-300 text-xs shadow-[0_0_30px_-12px_rgba(255,179,0,0.6)]">
          serverbee
        </div>
        <svg aria-hidden="true" focusable="false" height="32" viewBox="0 0 20 32" width="20">
          <path
            d="M10 2 L10 24 M4 18 L10 26 L16 18"
            fill="none"
            stroke="#ffb300"
            strokeLinecap="round"
            strokeWidth="2"
          />
        </svg>
        <div className="flex items-center gap-2 rounded-md border border-white/10 bg-white/[0.04] px-3 py-2 font-mono text-xs text-zinc-300">
          <span className="pulse-dot inline-block h-2 w-2 rounded-full bg-emerald-400" />
          systemd · active
        </div>
      </div>
    </figure>
  )
}
