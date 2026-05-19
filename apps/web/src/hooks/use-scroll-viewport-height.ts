import { useLayoutEffect, useRef, useState } from 'react'

/**
 * Measures the clientHeight of the nearest enclosing shadcn ScrollArea
 * viewport (the app's page-level scroll container) and keeps it in sync via
 * ResizeObserver. Use it to pin a region to the available height so it can
 * scroll internally instead of growing the outer page scroll.
 */
export function useScrollViewportHeight<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [height, setHeight] = useState<number | null>(null)

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) {
      return
    }
    const viewport = el.closest<HTMLElement>('[data-slot="scroll-area-viewport"]')
    if (!viewport) {
      return
    }
    const update = () => {
      const offsetTop = el.getBoundingClientRect().top - viewport.getBoundingClientRect().top
      const parent = el.parentElement
      const padBottom = parent ? Number.parseFloat(getComputedStyle(parent).paddingBottom) || 0 : 0
      setHeight(Math.max(0, viewport.clientHeight - offsetTop - padBottom))
    }
    update()
    const observer = new ResizeObserver(update)
    observer.observe(viewport)
    return () => observer.disconnect()
  }, [])

  return { ref, height }
}
