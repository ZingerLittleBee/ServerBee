import { Button as ButtonPrimitive } from '@base-ui/react/button'
import { cn } from '@serverbee/ui/lib/utils'
import type { VariantProps } from 'class-variance-authority'
import { buttonVariants } from './button-variants'

function Button({
  className,
  variant = 'default',
  size = 'default',
  ...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
  return <ButtonPrimitive className={cn(buttonVariants({ variant, size, className }))} data-slot="button" {...props} />
}

export { Button }
