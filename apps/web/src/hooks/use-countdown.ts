import { useEffect, useState } from 'react'

// Live seconds remaining until `expiresAtSecs` (Unix epoch seconds). Ticks every
// second; returns 0 once elapsed. `null` expiry → null (no countdown).
export function useCountdown(expiresAtSecs: number | null | undefined): number | null {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    if (expiresAtSecs == null) {
      return
    }
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000)
    return () => clearInterval(id)
  }, [expiresAtSecs])
  if (expiresAtSecs == null) {
    return null
  }
  return Math.max(0, expiresAtSecs - now)
}

export function formatCountdown(secs: number): string {
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600)
    const m = Math.floor((secs % 3600) / 60)
    return `${h}h ${m}m`
  }
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}
