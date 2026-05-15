import { Box } from 'lucide-react'

export function DockerStackAnim() {
  const containers = [
    { name: 'web', tag: 'caddy:2', cpu: '0.4%', delay: '0s' },
    { name: 'api', tag: 'rust:1.84', cpu: '1.2%', delay: '0.4s' },
    { name: 'cache', tag: 'redis:7', cpu: '0.1%', delay: '0.8s' }
  ]
  return (
    <div
      aria-label="Animated demo of Docker container management"
      className="flex h-full flex-col justify-center gap-1.5"
      role="img"
    >
      {containers.map((c) => (
        <div
          className="flex items-center gap-2 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 font-mono text-[11px]"
          key={c.name}
        >
          <Box className="h-3.5 w-3.5 shrink-0 text-cyan-300" />
          <span className="truncate text-zinc-200">{c.name}</span>
          <span className="truncate text-[10px] text-zinc-500">{c.tag}</span>
          <span className="ml-auto inline-flex items-center gap-1.5 text-[10px] text-emerald-300">
            <span
              className="pulse-dot inline-block h-1.5 w-1.5 rounded-full bg-emerald-400"
              style={{ animationDelay: c.delay }}
            />
            {c.cpu}
          </span>
        </div>
      ))}
    </div>
  )
}
