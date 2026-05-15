import { Bell } from 'lucide-react'

export function AlertBellAnim() {
  const channels = ['Webhook', 'Telegram', 'Bark', 'Email', 'APNs']
  return (
    <div
      aria-label="Animated demo of multi-channel alerts"
      className="flex h-full flex-col items-center justify-center gap-3"
      role="img"
    >
      <Bell className="bell-shake h-10 w-10 text-amber-300" />
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
    </div>
  )
}
