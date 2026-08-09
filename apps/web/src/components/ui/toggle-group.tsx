'use client'

import { Toggle as TogglePrimitive } from '@base-ui/react/toggle'
import { ToggleGroup as ToggleGroupPrimitive } from '@base-ui/react/toggle-group'
import type { VariantProps } from 'class-variance-authority'
import { MotionConfig, motion, useReducedMotion } from 'motion/react'
// biome-ignore lint/performance/noNamespaceImport: React namespace import required for createContext and CSSProperties
import * as React from 'react'
import { use, useId, useMemo } from 'react'
import { toggleVariants } from '@/components/ui/toggle-variants'
import { SPRING_INDICATOR } from '@/lib/ease'
import { cn } from '@/lib/utils'

interface ToggleGroupContextValue extends VariantProps<typeof toggleVariants> {
  animateIndicator: boolean
  layoutId: string
  orientation?: 'horizontal' | 'vertical'
  selectedValues: readonly string[]
  spacing?: number
}

const ToggleGroupContext = React.createContext<ToggleGroupContextValue>({
  animateIndicator: false,
  layoutId: '',
  orientation: 'horizontal',
  selectedValues: [],
  size: 'default',
  spacing: 0,
  variant: 'default'
})

function ToggleGroup({
  animated = true,
  children,
  className,
  multiple,
  orientation = 'horizontal',
  size,
  spacing = 0,
  value,
  variant,
  ...props
}: ToggleGroupPrimitive.Props &
  VariantProps<typeof toggleVariants> & {
    /** When false, the active indicator snaps with no layoutId spring. Default true. */
    animated?: boolean
    orientation?: 'horizontal' | 'vertical'
    spacing?: number
  }) {
  const layoutId = useId()
  const reduce = useReducedMotion()
  // Sliding indicator only for exclusive selection when animation is enabled.
  const animateIndicator = animated && multiple === false && !reduce
  const selectedKey = Array.isArray(value) ? value.map(String).join('\0') : ''
  const contextValue = useMemo(
    () => ({
      animateIndicator,
      layoutId,
      orientation,
      selectedValues: selectedKey.length > 0 ? selectedKey.split('\0') : [],
      size,
      spacing,
      variant
    }),
    [animateIndicator, layoutId, orientation, selectedKey, size, spacing, variant]
  )

  const group = (
    <ToggleGroupPrimitive
      className={cn(
        'group/toggle-group flex w-full flex-row items-center gap-[--spacing(var(--gap))] rounded-lg data-vertical:flex-col data-vertical:items-stretch data-[size=sm]:rounded-[min(var(--radius-md),10px)]',
        !animateIndicator && className
      )}
      data-orientation={orientation}
      data-size={size}
      data-slot="toggle-group"
      data-spacing={spacing}
      data-variant={variant}
      multiple={multiple}
      style={{ '--gap': spacing } as React.CSSProperties}
      value={value}
      {...props}
    >
      <ToggleGroupContext.Provider value={contextValue}>{children}</ToggleGroupContext.Provider>
    </ToggleGroupPrimitive>
  )

  if (!animateIndicator) {
    return group
  }

  return (
    <MotionConfig transition={SPRING_INDICATOR}>
      {/* layoutRoot scopes layoutId projection so scroll containers don't skew the glide. */}
      <motion.div className={cn('w-fit max-w-full', className)} layoutRoot>
        {group}
      </motion.div>
    </MotionConfig>
  )
}

function ToggleGroupItem({
  children,
  className,
  size = 'default',
  value,
  variant = 'default',
  ...props
}: TogglePrimitive.Props & VariantProps<typeof toggleVariants>) {
  const context = use(ToggleGroupContext)
  const itemValue = value == null ? '' : String(value)
  const isActive = itemValue.length > 0 && context.selectedValues.includes(itemValue)
  const showIndicator = context.animateIndicator && isActive

  return (
    <TogglePrimitive
      className={cn(
        'relative shrink-0 focus:z-10 focus-visible:z-10 group-data-[spacing=0]/toggle-group:rounded-none group-data-vertical/toggle-group:data-[spacing=0]:data-[variant=outline]:border-t-0 group-data-horizontal/toggle-group:data-[spacing=0]:data-[variant=outline]:border-l-0 group-data-[spacing=0]/toggle-group:px-2 group-data-horizontal/toggle-group:data-[spacing=0]:last:rounded-r-lg group-data-vertical/toggle-group:data-[spacing=0]:last:rounded-b-lg group-data-vertical/toggle-group:data-[spacing=0]:data-[variant=outline]:first:border-t group-data-horizontal/toggle-group:data-[spacing=0]:data-[variant=outline]:first:border-l group-data-vertical/toggle-group:data-[spacing=0]:first:rounded-t-lg group-data-horizontal/toggle-group:data-[spacing=0]:first:rounded-l-lg',
        // Indicator owns the fill so layoutId can glide without fighting the CSS on-state.
        context.animateIndicator && 'data-[state=on]:bg-transparent',
        toggleVariants({
          variant: context.variant || variant,
          size: context.size || size
        }),
        className
      )}
      data-size={context.size || size}
      data-slot="toggle-group-item"
      data-spacing={context.spacing}
      data-variant={context.variant || variant}
      value={value}
      {...props}
    >
      {showIndicator ? (
        <motion.span className="absolute inset-0 z-0 rounded-[inherit] bg-muted" layoutId={context.layoutId} />
      ) : null}
      <span className="relative z-10 inline-flex items-center justify-center gap-1">{children}</span>
    </TogglePrimitive>
  )
}

export { ToggleGroup, ToggleGroupItem }
