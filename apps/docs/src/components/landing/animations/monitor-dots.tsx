export function MonitorDotsAnim() {
  const probes = [
    { name: 'SSL', meta: 'expires in 73d' },
    { name: 'DNS', meta: 'A · CNAME' },
    { name: 'HTTP', meta: '200 · keyword OK' },
    { name: 'TCP', meta: ':443 · 18 ms' },
    { name: 'WHOIS', meta: 'renews 2027-04' }
  ]
  return (
    <figure
      aria-label="Animated demo of service monitors"
      className="m-0 grid h-full grid-cols-1 gap-1.5 font-mono text-xs sm:grid-cols-2"
    >
      {probes.map((p, i) => (
        <div className="flex items-center justify-between rounded-md bg-white/[0.03] px-3 py-1.5" key={p.name}>
          <div className="flex items-center gap-2">
            <span
              className="pulse-dot inline-block h-2 w-2 rounded-full bg-emerald-400"
              style={{ animationDelay: `${i * 0.35}s` }}
            />
            <span className="text-zinc-200">{p.name}</span>
          </div>
          <span className="text-[10px] text-zinc-500">{p.meta}</span>
        </div>
      ))}
    </figure>
  )
}
