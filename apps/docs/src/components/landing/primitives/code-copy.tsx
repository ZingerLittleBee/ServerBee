import { Check, Copy } from 'lucide-react'
import { useState } from 'react'

import { cn } from '@/lib/cn'

export function CodeCopy({ command, label, className }: { command: string; label?: string; className?: string }) {
  const [copied, setCopied] = useState(false)

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // Clipboard API unavailable (e.g. insecure context). The command stays
      // visible and selectable so users can still copy it manually.
    }
  }

  return (
    <div
      className={cn(
        'group flex w-full max-w-3xl items-start gap-3 rounded-xl border border-white/10 bg-white/[0.04] px-4 py-3 font-mono text-[13px] shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] backdrop-blur',
        className
      )}
    >
      {label ? (
        <span className="mt-0.5 shrink-0 select-none rounded-md bg-amber-400/15 px-2 py-0.5 text-amber-300 text-xs">
          {label}
        </span>
      ) : null}
      <code className="min-w-0 flex-1 break-all text-zinc-200 leading-relaxed">
        <span className="text-amber-400">$ </span>
        {command}
      </code>
      <button
        aria-label="Copy install command"
        className="shrink-0 rounded-md p-1.5 text-zinc-400 transition hover:bg-white/10 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400"
        onClick={onCopy}
        type="button"
      >
        {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
      </button>
    </div>
  )
}
