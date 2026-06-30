export function TerminalDemoAnim() {
  return (
    <figure
      aria-label="Animated demo of the web terminal"
      className="m-0 flex h-full flex-col overflow-hidden rounded-lg border border-white/10 bg-black/40"
    >
      <header className="flex items-center gap-1.5 border-white/5 border-b px-3 py-2">
        <span className="h-2.5 w-2.5 rounded-full bg-red-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-amber-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-emerald-400/70" />
        <span className="ml-2 font-mono text-[10px] text-zinc-500">edge-tokyo-01 ~ #</span>
      </header>
      <pre className="flex-1 px-3 py-3 font-mono text-xs text-zinc-300 leading-relaxed">
        <span className="text-zinc-500">$ </span>
        <span className="typewriter text-amber-300">serverbee agent --version</span>
        {'\n'}
        <span className="text-zinc-300">serverbee-agent 0.3.0 (linux/arm64)</span>
        {'\n'}
        <span className="text-zinc-500">features: </span>
        <span className="text-emerald-300">terminal,file,docker,ping,upgrade</span>
        {'\n\n'}
        <span className="text-zinc-500">$ </span>
        <span className="typewriter text-amber-300">serverbee server status</span>
        {'\n'}
        <span className="text-zinc-300">● serverbee-server.service · </span>
        <span className="text-emerald-300">active (running)</span>
        {'\n'}
        <span className="text-zinc-500"> └─ uptime: 14d · agents: 7 · alerts: 0</span>
        {'\n\n'}
        <span className="text-zinc-500">$ </span>
        <span className="blink text-zinc-500">▍</span>
      </pre>
    </figure>
  )
}
