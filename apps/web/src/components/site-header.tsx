import { createContext, type ReactNode, useContext, useState } from 'react'
import { createPortal } from 'react-dom'
import { Separator } from '@/components/ui/separator'
import { SidebarTrigger } from '@/components/ui/sidebar'

interface SiteHeaderPortalsContextValue {
  actionsTarget: HTMLDivElement | null
  leadingTarget: HTMLDivElement | null
  setActionsTarget: (target: HTMLDivElement | null) => void
  setLeadingTarget: (target: HTMLDivElement | null) => void
}

const SiteHeaderPortalsContext = createContext<SiteHeaderPortalsContextValue | null>(null)

/** @deprecated Prefer SiteHeaderPortalsProvider — alias kept for existing call sites. */
export function SiteHeaderActionsProvider({ children }: { children: ReactNode }) {
  const [leadingTarget, setLeadingTarget] = useState<HTMLDivElement | null>(null)
  const [actionsTarget, setActionsTarget] = useState<HTMLDivElement | null>(null)

  return (
    <SiteHeaderPortalsContext value={{ actionsTarget, leadingTarget, setActionsTarget, setLeadingTarget }}>
      {children}
    </SiteHeaderPortalsContext>
  )
}

/** Portal content into the header leading slot (after sidebar trigger, before title). */
export function SiteHeaderLeading({ children }: { children: ReactNode }) {
  const context = useContext(SiteHeaderPortalsContext)
  return context?.leadingTarget ? createPortal(children, context.leadingTarget) : null
}

/** Portal content into the header trailing actions slot (far right). */
export function SiteHeaderActions({ children }: { children: ReactNode }) {
  const context = useContext(SiteHeaderPortalsContext)
  return context?.actionsTarget ? createPortal(children, context.actionsTarget) : null
}

interface SiteHeaderProps {
  children: ReactNode
}

export function SiteHeader({ children }: SiteHeaderProps) {
  const context = useContext(SiteHeaderPortalsContext)

  return (
    <header className="flex h-(--header-height) shrink-0 items-center gap-2 border-b transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height)">
      <div className="flex w-full min-w-0 items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator className="mx-2 h-4 data-vertical:self-auto" orientation="vertical" />
        <div className="flex min-w-0 shrink-0 items-center" ref={context?.setLeadingTarget} />
        {children}
        <div className="ml-auto flex min-w-0 shrink-0 items-center" ref={context?.setActionsTarget} />
      </div>
    </header>
  )
}
