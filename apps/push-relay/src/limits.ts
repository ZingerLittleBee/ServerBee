const ipPattern = /^[0-9a-fA-F:.]+$/

/** Best-effort isolate-local windows, never a global quota or access control. */
export class WindowLimiter {
  private readonly entries = new Map<string, { window: number; count: number }>()

  private readonly maxEntries: number
  private readonly windowMs: number

  constructor(maxEntries = 4096, windowMs = 60_000) {
    this.maxEntries = maxEntries
    this.windowMs = windowMs
  }

  get size(): number {
    return this.entries.size
  }

  allow(key: string, limit: number, now: number): boolean {
    const window = Math.floor(now / this.windowMs)
    const existing = this.entries.get(key)
    if (existing?.window === window) {
      if (existing.count >= limit) {
        return false
      }
      existing.count += 1
      return true
    }
    if (!existing && this.entries.size >= this.maxEntries) {
      for (const [entryKey, entry] of this.entries) {
        if (entry.window !== window) {
          this.entries.delete(entryKey)
        }
      }
      // Reject unseen keys when all slots are live; never evict to bypass limits.
      if (this.entries.size >= this.maxEntries) {
        return false
      }
    }
    this.entries.set(key, { window, count: 1 })
    return true
  }
}

/** Cloudflare overwrites this header at ingress. Never trust X-Forwarded-For. */
export function cloudflareClientIp(request: Request): string {
  const ip = request.headers.get('CF-Connecting-IP')
  return ip && ip.length <= 45 && ipPattern.test(ip) ? ip : 'unknown'
}
