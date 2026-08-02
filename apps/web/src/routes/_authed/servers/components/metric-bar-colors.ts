export function getBarColor(pct: number): string {
  if (pct > 90) {
    return 'bg-status-danger'
  }
  if (pct > 70) {
    return 'bg-status-warning'
  }
  return 'bg-status-healthy'
}

export function getBarTextColor(pct: number): string {
  if (pct > 90) {
    return 'text-status-danger-text'
  }
  if (pct > 70) {
    return 'text-status-warning-text'
  }
  return 'text-foreground'
}
