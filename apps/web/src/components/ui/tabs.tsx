// Tabs keep the previous shadcn look with an instant active indicator (no glide).

import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from 'react'
import { cn } from '@/lib/utils'

type TabsListVariant = 'default' | 'line'

interface TabsContextValue {
  setValue: (value: string) => void
  value: string
}

const TabsContext = createContext<TabsContextValue | null>(null)
const TabsListVariantContext = createContext<TabsListVariant>('default')

function useTabs() {
  const ctx = useContext(TabsContext)
  if (!ctx) {
    throw new Error('Tabs.* must be used inside <Tabs>')
  }
  return ctx
}

export function Tabs({
  children,
  className,
  defaultValue,
  onValueChange,
  value
}: {
  children: ReactNode
  className?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  value?: string
}) {
  const [internal, setInternal] = useState(defaultValue ?? '')
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
  const contextValue = useMemo(() => ({ setValue, value: current }), [current, setValue])

  return (
    <TabsContext.Provider value={contextValue}>
      <div className={cn('flex flex-col gap-2', className)} data-slot="tabs">
        {children}
      </div>
    </TabsContext.Provider>
  )
}

export function TabsList({
  children,
  className,
  variant = 'default'
}: {
  children: ReactNode
  className?: string
  variant?: TabsListVariant
}) {
  return (
    <TabsListVariantContext.Provider value={variant}>
      <div
        className={cn(
          'inline-flex h-8 w-fit items-center justify-center rounded-lg p-[3px] text-muted-foreground',
          variant === 'default' && 'bg-muted',
          variant === 'line' && 'gap-1 rounded-none bg-transparent',
          className
        )}
        data-slot="tabs-list"
        data-variant={variant}
        role="tablist"
      >
        {children}
      </div>
    </TabsListVariantContext.Provider>
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
  const { setValue, value: current } = useTabs()
  const listVariant = useContext(TabsListVariantContext)
  const active = current === value

  if (listVariant === 'line') {
    return (
      <button
        aria-selected={active}
        className={cn(
          'relative inline-flex h-[calc(100%-1px)] flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-md border border-transparent px-1.5 py-0.5 font-medium text-sm',
          'text-foreground/60 transition-none hover:text-foreground',
          'focus-visible:border-ring focus-visible:outline-1 focus-visible:outline-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
          'disabled:pointer-events-none disabled:opacity-50',
          'dark:text-muted-foreground dark:hover:text-foreground',
          "[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
          active && 'text-foreground dark:text-foreground',
          className
        )}
        data-slot="tabs-trigger"
        onClick={() => setValue(value)}
        role="tab"
        type="button"
      >
        {children}
        {active ? (
          <span className={cn('absolute inset-x-0 bottom-[-5px] h-0.5 bg-foreground', indicatorClassName)} />
        ) : null}
      </button>
    )
  }

  return (
    <div className="relative h-[calc(100%-1px)] flex-1">
      {active ? (
        <span
          className={cn(
            'absolute inset-0 rounded-md bg-background shadow-sm',
            'dark:border dark:border-input dark:bg-input/30',
            indicatorClassName
          )}
        />
      ) : null}
      <button
        aria-selected={active}
        className={cn(
          'relative z-10 inline-flex h-full w-full items-center justify-center gap-1.5 whitespace-nowrap rounded-md border border-transparent px-1.5 py-0.5 font-medium text-sm',
          'bg-transparent text-foreground/60 transition-none hover:text-foreground',
          'focus-visible:border-ring focus-visible:outline-1 focus-visible:outline-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
          'disabled:pointer-events-none disabled:opacity-50',
          'dark:text-muted-foreground dark:hover:text-foreground',
          "[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
          active && 'text-foreground dark:text-foreground',
          className
        )}
        data-slot="tabs-trigger"
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
  const active = current === value

  // Keep inactive panels mounted (hidden) so chart/form state survives tab switches.
  return (
    <div className={cn('flex-1 text-sm outline-none', className)} data-slot="tabs-content" hidden={!active}>
      {children}
    </div>
  )
}
