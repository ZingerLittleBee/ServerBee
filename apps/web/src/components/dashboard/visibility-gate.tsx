import { type ReactNode, useEffect, useRef, useState } from 'react'

interface VisibilityGateProps {
  children: ReactNode
  /** When true, render children immediately without any gating. Use this for
   *  the dashboard edit/customize flow where every widget must be visible so
   *  the user can identify and rearrange them, and for auto-height widgets
   *  whose grid cell size depends on real measured content. */
  disabled?: boolean
  fallback?: ReactNode
  rootMargin?: string
}

/**
 * Defers rendering of {children} until the wrapper enters the viewport (or
 * comes within `rootMargin`). Used to avoid paying the React mount cost of
 * off-screen dashboard widgets — recharts widgets call getBBox during mount
 * which triggers forced reflow.
 *
 * When `disabled` is true the children render immediately, but the same div
 * wrapper is preserved so toggling `disabled` (e.g. entering/exiting edit
 * mode) does not change the JSX structure — otherwise React would unmount
 * and remount every child and discard their state. Callers must disable for
 * any layout that depends on children being measured (auto-height grid
 * cells) or interacted with (edit mode drag/drop).
 */
export function VisibilityGate({ children, disabled = false, fallback, rootMargin = '200px' }: VisibilityGateProps) {
  const ref = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(disabled)

  useEffect(() => {
    if (disabled) {
      setVisible(true)
      return
    }
    const el = ref.current
    if (!el) {
      return
    }
    if (typeof IntersectionObserver === 'undefined') {
      setVisible(true)
      return
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setVisible(true)
            observer.disconnect()
            return
          }
        }
      },
      { rootMargin }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [disabled, rootMargin])

  return (
    <div className="h-full w-full" ref={ref}>
      {visible || disabled ? children : fallback}
    </div>
  )
}
