import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

/**
 * Borderless widget surface: a hairline ring plus a soft drop in light mode,
 * a faint ring in dark mode. Layout rhythm comes from spacing, not borders.
 */
export const DASHBOARD_CARD_CLASS_NAME =
  'min-w-0 rounded-2xl shadow-[0_0_0_1px_rgb(0_0_0/0.06),0_1px_3px_rgb(0_0_0/0.05)] dark:shadow-[0_0_0_1px_rgb(255_255_255/0.07)]'

export function DashboardCard({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn(DASHBOARD_CARD_CLASS_NAME, className)} data-slot="dashboard-card" {...props} />
}
