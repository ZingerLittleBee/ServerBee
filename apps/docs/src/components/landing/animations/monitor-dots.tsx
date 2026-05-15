export function MonitorDotsAnim() {
  const probes = ['SSL', 'DNS', 'HTTP', 'TCP', 'WHOIS']
  return (
    <div
      aria-label="Animated demo of service monitors"
      className="flex h-full flex-col justify-center gap-2 font-mono text-xs"
      role="img"
    >
      {probes.map((p, i) => (
        <div className="flex items-center justify-between rounded-md bg-white/[0.03] px-3 py-1.5" key={p}>
          <span className="text-zinc-300">{p}</span>
          <span
            className="pulse-dot inline-block h-2 w-2 rounded-full bg-emerald-400"
            style={{ animationDelay: `${i * 0.35}s` }}
          />
        </div>
      ))}
    </div>
  )
}
