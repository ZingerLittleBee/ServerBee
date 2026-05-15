import { Box } from 'lucide-react'

export function DockerStackAnim() {
  const containers = [
    { name: 'web', tag: 'caddy:2', delay: '0s' },
    { name: 'api', tag: 'rust:1.84', delay: '0.4s' },
    { name: 'cache', tag: 'redis:7', delay: '0.8s' }
  ]
  return (
    <div aria-label="Animated demo of Docker container management" className="space-y-2" role="img">
      {containers.map((c) => (
        <div
          className="flex items-center gap-3 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2 font-mono text-xs"
          key={c.name}
        >
          <Box className="h-4 w-4 text-cyan-300" />
          <span className="text-zinc-200">{c.name}</span>
          <span className="text-zinc-500">{c.tag}</span>
          <span className="ml-auto inline-flex items-center gap-1.5 text-emerald-300">
            <span
              className="pulse-dot inline-block h-1.5 w-1.5 rounded-full bg-emerald-400"
              style={{ animationDelay: c.delay }}
            />
            running
          </span>
        </div>
      ))}
    </div>
  )
}
