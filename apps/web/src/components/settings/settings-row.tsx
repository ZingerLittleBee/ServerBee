import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

export function SettingsSection({
  children,
  className,
  footer,
  title
}: {
  children: ReactNode
  className?: string
  footer?: ReactNode
  title: string
}) {
  return (
    <section className={className}>
      <h2 className="mb-2 px-1 font-medium text-muted-foreground text-sm">{title}</h2>
      <div className="divide-y rounded-lg border bg-card">{children}</div>
      {footer && <div className="mt-2 px-1 text-muted-foreground text-xs">{footer}</div>}
    </section>
  )
}

export function SettingsRow({
  action,
  description,
  icon,
  meta,
  title
}: {
  action?: ReactNode
  description?: ReactNode
  icon: ReactNode
  meta?: ReactNode
  title: ReactNode
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-3.5">
      <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted/60 text-muted-foreground">
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-sm">{title}</div>
        {description && <div className="truncate text-muted-foreground text-xs">{description}</div>}
      </div>
      {meta && <div className={cn('shrink-0 text-muted-foreground text-xs')}>{meta}</div>}
      {action && <div className="shrink-0">{action}</div>}
    </div>
  )
}
