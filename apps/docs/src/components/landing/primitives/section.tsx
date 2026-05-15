import type { PropsWithChildren } from 'react'

import { cn } from '@/lib/cn'

export function Section({ id, className, children }: PropsWithChildren<{ id?: string; className?: string }>) {
  return (
    <section className={cn('relative w-full border-white/5 border-b px-6 py-20 sm:py-24 lg:py-28', className)} id={id}>
      <div className="mx-auto w-full max-w-6xl">{children}</div>
    </section>
  )
}
