import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export const DASHBOARD_CARD_CLASS_NAME = 'min-w-0 rounded-xl border border-border/70 shadow-sm'

export function DashboardCard({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn(DASHBOARD_CARD_CLASS_NAME, className)} data-slot="dashboard-card" {...props} />
}
