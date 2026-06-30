export function getBarColor(pct: number): string {
  if (pct > 90) {
    return 'bg-red-500'
  }
  if (pct > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

export function getBarTextColor(pct: number): string {
  if (pct > 90) {
    return 'text-red-600 dark:text-red-400'
  }
  if (pct > 70) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-foreground'
}
