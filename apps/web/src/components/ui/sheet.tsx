import { Dialog as DialogPrimitive } from '@base-ui/react/dialog'
import { XIcon } from 'lucide-react'
import type * as React from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

function Sheet({ ...props }: DialogPrimitive.Root.Props) {
  return <DialogPrimitive.Root data-slot="sheet" {...props} />
}

function SheetTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
  return <DialogPrimitive.Trigger data-slot="sheet-trigger" {...props} />
}

function SheetPortal({ ...props }: DialogPrimitive.Portal.Props) {
  return <DialogPrimitive.Portal data-slot="sheet-portal" {...props} />
}

function SheetClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot="sheet-close" {...props} />
}

function SheetOverlay({ className, ...props }: DialogPrimitive.Backdrop.Props) {
  return (
    <DialogPrimitive.Backdrop
      className={cn(
        'data-open:fade-in-0 data-closed:fade-out-0 fixed inset-0 isolate z-50 bg-black/50 duration-200 data-closed:animate-out data-open:animate-in supports-backdrop-filter:backdrop-blur-xs',
        className
      )}
      data-slot="sheet-overlay"
      {...props}
    />
  )
}

function SheetContent({
  className,
  children,
  side = 'left',
  ...props
}: DialogPrimitive.Popup.Props & {
  side?: 'bottom' | 'left' | 'right' | 'top'
}) {
  const { t } = useTranslation('common')
  return (
    <SheetPortal>
      <SheetOverlay />
      <DialogPrimitive.Popup
        className={cn(
          'fixed z-50 flex flex-col bg-background shadow-lg outline-none ring-1 ring-foreground/10 duration-200',
          'data-closed:animate-out data-open:animate-in',
          side === 'left' &&
            'data-open:slide-in-from-left data-closed:slide-out-to-left inset-y-0 left-0 w-72 border-r',
          side === 'right' &&
            'data-open:slide-in-from-right data-closed:slide-out-to-right inset-y-0 right-0 w-72 border-l',
          side === 'top' && 'data-open:slide-in-from-top data-closed:slide-out-to-top inset-x-0 top-0 h-auto border-b',
          side === 'bottom' &&
            'data-open:slide-in-from-bottom data-closed:slide-out-to-bottom inset-x-0 bottom-0 h-auto border-t',
          className
        )}
        data-slot="sheet-content"
        {...props}
      >
        {children}
        <DialogPrimitive.Close
          data-slot="sheet-close"
          render={<Button className="absolute top-2 right-2" size="icon-sm" variant="ghost" />}
        >
          <XIcon />
          <span className="sr-only">{t('a11y.close')}</span>
        </DialogPrimitive.Close>
      </DialogPrimitive.Popup>
    </SheetPortal>
  )
}

function SheetHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return <div className={cn('flex flex-col gap-2 p-4', className)} data-slot="sheet-header" {...props} />
}

function SheetTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      className={cn('font-medium text-base leading-none', className)}
      data-slot="sheet-title"
      {...props}
    />
  )
}

function SheetDescription({ className, ...props }: DialogPrimitive.Description.Props) {
  return (
    <DialogPrimitive.Description
      className={cn('text-muted-foreground text-sm', className)}
      data-slot="sheet-description"
      {...props}
    />
  )
}

export {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetOverlay,
  SheetPortal,
  SheetTitle,
  SheetTrigger
}
