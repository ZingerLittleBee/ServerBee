import { Bell } from 'lucide-react'

export function AlertBellAnim() {
  const channels = ['Webhook', 'Telegram', 'Bark', 'Email', 'APNs']
  return (
    <figure
      aria-label="Animated demo of multi-channel alerts"
      className="m-0 flex h-full flex-col items-center justify-center gap-3"
    >
      <div className="relative">
        <Bell className="bell-shake h-9 w-9 text-amber-300" />
        <span className="absolute -top-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-red-500 ring-2 ring-zinc-950" />
      </div>
      <div className="flex flex-wrap justify-center gap-1.5">
        {channels.map((c, i) => (
          <span
            className="fade-cycle rounded-full border border-white/10 bg-white/[0.04] px-2 py-0.5 font-mono text-[10px] text-zinc-300"
            key={c}
            style={{ animationDelay: `${i * 0.3}s` }}
          >
            {c}
          </span>
        ))}
      </div>
    </figure>
  )
}
