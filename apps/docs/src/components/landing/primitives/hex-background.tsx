import { cn } from '@/lib/cn'

export function HexBackground({ className }: { className?: string }) {
  return (
    <div aria-hidden className={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}>
      <div className="hex-bg absolute inset-0" />
      <svg
        aria-hidden="true"
        className="hex-drift absolute inset-x-0 -top-10 h-[140%] w-full opacity-[0.18]"
        focusable="false"
        viewBox="0 0 800 800"
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          <pattern height="46" id="hex" patternUnits="userSpaceOnUse" width="40">
            <polygon
              fill="none"
              points="20,2 38,12 38,34 20,44 2,34 2,12"
              stroke="currentColor"
              strokeOpacity="0.35"
              strokeWidth="0.6"
            />
          </pattern>
          <linearGradient id="hex-fade" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="white" stopOpacity="0.18" />
            <stop offset="100%" stopColor="white" stopOpacity="0" />
          </linearGradient>
          <mask id="hex-mask">
            <rect fill="url(#hex-fade)" height="100%" width="100%" />
          </mask>
        </defs>
        <rect className="text-amber-300" fill="url(#hex)" height="100%" mask="url(#hex-mask)" width="100%" />
      </svg>
    </div>
  )
}
