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

    let rafId: number | null = null
    let lastHeight = -1

    // Read layout in a rAF callback so it batches with the browser's natural
    // paint cycle instead of forcing a synchronous reflow inside the
    // ResizeObserver dispatch (which fires before paint).
    const schedule = () => {
      if (rafId !== null) {
        return
      }
      rafId = requestAnimationFrame(() => {
        rafId = null
        const elRect = el.getBoundingClientRect()
        const viewportRect = viewport.getBoundingClientRect()
        const offsetTop = elRect.top - viewportRect.top
        const parent = el.parentElement
        const padBottom = parent ? Number.parseFloat(getComputedStyle(parent).paddingBottom) || 0 : 0
        const next = Math.max(0, viewport.clientHeight - offsetTop - padBottom)
        if (next !== lastHeight) {
          lastHeight = next
          setHeight(next)
        }
      })
    }

    schedule()
    const observer = new ResizeObserver(schedule)
    observer.observe(viewport)
    return () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId)
      }
      observer.disconnect()
    }
  }, [])

  return { ref, height }
}
