import { Button as ButtonPrimitive } from '@base-ui/react/button'
import type { VariantProps } from 'class-variance-authority'

import { cn } from '@/lib/utils'
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
