// beui.dev/components/motion/tabs — pill / segment / underline with spring layoutId indicator.

import { MotionConfig, motion, type Transition, useReducedMotion } from 'motion/react'
import { createContext, type ReactNode, useCallback, useContext, useId, useMemo, useState } from 'react'
import { EASE_OUT } from '@/lib/ease'
import { cn } from '@/lib/utils'

type Variant = 'pill' | 'underline' | 'segment'

interface TabsContextValue {
  layoutId: string
  setValue: (value: string) => void
  value: string
  variant: Variant
}

const TabsContext = createContext<TabsContextValue | null>(null)

function useTabs() {
  const ctx = useContext(TabsContext)
  if (!ctx) {
    throw new Error('Tabs.* must be used inside <Tabs>')
  }
  return ctx
}

// Weighty spring for the active-tab indicator: a touch of overshoot so it
// settles with life instead of snapping.
const indicatorTransition: Transition = {
  type: 'spring',
  stiffness: 170,
  damping: 24,
  mass: 1.2
}

export function Tabs({
  children,
  className,
  defaultValue,
  onValueChange,
  value,
  variant = 'pill'
}: {
  children: ReactNode
  className?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  value?: string
  variant?: Variant
}) {
  const [internal, setInternal] = useState(defaultValue ?? '')
  const layoutId = useId()
  const reduce = useReducedMotion()
  const controlled = value !== undefined
  const current = controlled ? value : internal
  const setValue = useCallback(
    (next: string) => {
      if (!controlled) {
        setInternal(next)
      }
      onValueChange?.(next)
    },
    [controlled, onValueChange]
  )
  const contextValue = useMemo(
    () => ({ layoutId, setValue, value: current, variant }),
    [current, layoutId, setValue, variant]
  )

  return (
    <MotionConfig transition={reduce ? { duration: 0 } : indicatorTransition}>
      <TabsContext.Provider value={contextValue}>
        {/* layoutRoot: the indicator's layoutId measures in page coordinates, so
            inside fixed/scrolled containers it would replay scroll offsets as
            movement. The pill only ever travels within the list, so scoping
            projection to the Tabs wrapper is always correct. */}
        <motion.div className={cn('flex flex-col', className)} layoutRoot>
          {children}
        </motion.div>
      </TabsContext.Provider>
    </MotionConfig>
  )
}

const listClasses: Record<Variant, string> = {
  // bg-muted (not bg-card) so the track stays visible when tabs sit on a card surface.
  pill: 'inline-flex items-center gap-1 rounded-full bg-muted p-1',
  underline: 'inline-flex items-center gap-1 border-border border-b',
  segment: 'inline-flex items-center gap-0 rounded-lg bg-muted p-0.5'
}

export function TabsList({ children, className }: { children: ReactNode; className?: string }) {
  const { variant } = useTabs()
  return (
    <div className={cn(listClasses[variant], className)} role="tablist">
      {children}
    </div>
  )
}

export function TabsTrigger({
  children,
  className,
  indicatorClassName,
  value
}: {
  children: ReactNode
  className?: string
  indicatorClassName?: string
  value: string
}) {
  const { layoutId, setValue, value: current, variant } = useTabs()
  const active = current === value

  if (variant === 'underline') {
    return (
      <button
        aria-selected={active}
        className={cn(
          'relative isolate -mb-px inline-flex min-h-11 items-center px-3 pt-1 pb-2.5 font-medium text-sm transition-colors',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50',
          active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
          className
        )}
        onClick={() => setValue(value)}
        role="tab"
        type="button"
      >
        {children}
        {active ? (
          <motion.span
            className={cn('absolute right-0 -bottom-px left-0 h-px bg-primary', indicatorClassName)}
            layoutId={layoutId}
          />
        ) : null}
      </button>
    )
  }

  const radius = variant === 'pill' ? 'rounded-full' : 'rounded-md'

  return (
    <div className="relative">
      {active ? (
        <motion.span
          className={cn('absolute inset-0 bg-primary', radius, indicatorClassName)}
          layoutId={layoutId}
          style={{ borderRadius: variant === 'pill' ? 9999 : 8 }}
        />
      ) : null}
      <button
        aria-selected={active}
        className={cn(
          'relative z-10 inline-flex items-center justify-center whitespace-nowrap bg-transparent px-3.5 py-1.5 font-medium text-sm outline-none',
          'transition-colors focus-visible:ring-2 focus-visible:ring-ring/50',
          active ? 'text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
          radius,
          className
        )}
        onClick={() => setValue(value)}
        role="tab"
        type="button"
      >
        {children}
      </button>
    </div>
  )
}

export function TabsContent({
  children,
  className,
  value
}: {
  children: ReactNode
  className?: string
  value: string
}) {
  const { value: current } = useTabs()
  const reduce = useReducedMotion()
  const active = current === value

  // Inactive panels stay mounted but hidden so their state (forms, charts) is
  // preserved and content remains available to assistive tech / crawlers.
  if (!active) {
    return (
      <div className={className} hidden>
        {children}
      </div>
    )
  }

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className={cn('mt-4', className)}
      initial={{ opacity: 0, y: reduce ? 0 : 4 }}
      key={value}
      transition={{ duration: 0.18, ease: EASE_OUT }}
    >
      {children}
    </motion.div>
  )
}
