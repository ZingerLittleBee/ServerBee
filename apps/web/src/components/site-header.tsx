import { createContext, type ReactNode, useContext, useState } from 'react'
import { createPortal } from 'react-dom'
import { Separator } from '@/components/ui/separator'
import { SidebarTrigger } from '@/components/ui/sidebar'

interface SiteHeaderActionsContextValue {
  setTarget: (target: HTMLDivElement | null) => void
  target: HTMLDivElement | null
}

const SiteHeaderActionsContext = createContext<SiteHeaderActionsContextValue | null>(null)

export function SiteHeaderActionsProvider({ children }: { children: ReactNode }) {
  const [target, setTarget] = useState<HTMLDivElement | null>(null)

  return <SiteHeaderActionsContext value={{ setTarget, target }}>{children}</SiteHeaderActionsContext>
}

export function SiteHeaderActions({ children }: { children: ReactNode }) {
  const context = useContext(SiteHeaderActionsContext)

  return context?.target ? createPortal(children, context.target) : null
}

interface SiteHeaderProps {
  children: ReactNode
}

export function SiteHeader({ children }: SiteHeaderProps) {
  const context = useContext(SiteHeaderActionsContext)

  return (
    <header className="flex h-(--header-height) shrink-0 items-center gap-2 border-b transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height)">
      <div className="flex w-full min-w-0 items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator className="mx-2 h-4 data-vertical:self-auto" orientation="vertical" />
        {children}
        <div className="ml-auto flex min-w-0 shrink-0 items-center" ref={context?.setTarget} />
      </div>
    </header>
  )
}
