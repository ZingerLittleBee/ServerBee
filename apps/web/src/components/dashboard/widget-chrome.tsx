import type { ComponentProps, ReactNode } from 'react'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/lib/utils'

/** Inner padding shared by every built-in widget card. */
export const WIDGET_PADDING_CLASS_NAME = 'p-5'

interface WidgetHeaderProps {
  /** Right-aligned secondary text or controls (counts, ranges, legends). */
  aside?: ReactNode
  className?: string
  /** Scope of the reading, e.g. the server name. Rendered after a middle dot. */
  context?: ReactNode
  title: ReactNode
}

/**
 * Title row for built-in widgets: a quiet label instead of a bold heading, so
 * the reading below it carries the visual weight.
 */
export function WidgetHeader({ aside, className, context, title }: WidgetHeaderProps) {
  return (
    <div className={cn('flex min-w-0 items-baseline gap-3', className)}>
      <h3 className="min-w-0 truncate font-medium text-[13px] text-foreground/75 leading-5">
        {title}
        {context ? <span className="font-normal text-muted-foreground"> · {context}</span> : null}
      </h3>
      {aside ? <div className="ml-auto shrink-0 text-muted-foreground text-xs leading-5">{aside}</div> : null}
    </div>
  )
}

const VALUE_UNIT_RE = /^([+\-−]?\d[\d.,]*)(.*)$/

/** Split a formatted reading such as `75.3%` or `1.1 MB/s` into number and unit. */
export function splitValueUnit(formatted: string): { unit: string; value: string } {
  const match = VALUE_UNIT_RE.exec(formatted)
  if (!match) {
    return { value: formatted, unit: '' }
  }
  return { value: match[1], unit: match[2] }
}

interface WidgetValueProps extends Omit<ComponentProps<'p'>, 'children'> {
  /** Pre-formatted reading; a leading number keeps full size, the rest shrinks. */
  formatted: string
  unitClassName?: string
}

/**
 * Large tabular reading with a de-emphasized unit. Size comes from the caller's
 * `text-*` class; the unit scales with it.
 */
export function WidgetValue({ className, formatted, unitClassName, ...props }: WidgetValueProps) {
  const { value, unit } = splitValueUnit(formatted)
  return (
    <p className={cn('truncate font-semibold tabular-nums leading-none tracking-[-0.04em]', className)} {...props}>
      {value}
      {unit ? (
        <span className={cn('font-medium text-[0.55em] text-muted-foreground tracking-[-0.01em]', unitClassName)}>
          {unit}
        </span>
      ) : null}
    </p>
  )
}

interface WidgetMessageProps {
  className?: string
  description?: ReactNode
  icon?: ReactNode
  title: ReactNode
}

/** Centered empty / status message that states what the absence means. */
export function WidgetMessage({ className, description, icon, title }: WidgetMessageProps) {
  return (
    <div className={cn('flex flex-1 flex-col items-center justify-center gap-1.5 px-4 text-center', className)}>
      {icon ? <div className="mb-1 text-muted-foreground">{icon}</div> : null}
      <p className="font-medium text-foreground/80 text-sm">{title}</p>
      {description ? <p className="max-w-64 text-muted-foreground text-xs">{description}</p> : null}
    </div>
  )
}

/** Loading placeholder for chart widgets: a header line above the chart body. */
export function WidgetSkeleton() {
  return (
    <DashboardCard className={cn('flex h-full flex-col gap-4 bg-card', WIDGET_PADDING_CLASS_NAME)}>
      <Skeleton className="h-4 w-32" />
      <Skeleton className="flex-1 rounded-xl" />
    </DashboardCard>
  )
}
