import { type ComponentProps, useEffect, useRef, useState } from 'react'

/**
 * fumadocs' table, which scrolls sideways when it is wider than the page. While it does, it takes focus, so the
 * keyboard can scroll it as it scrolls fumadocs' code blocks: Chrome and Firefox make such a scroll container
 * focusable on their own, Safari does not.
 */
export function Table(props: ComponentProps<'table'>) {
  const container = useRef<HTMLDivElement>(null)
  const [scrolls, setScrolls] = useState(false)
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    const observer = new ResizeObserver(() => setScrolls(element.scrollWidth > element.clientWidth))
    observer.observe(element)
    // The table can widen without its container resizing, as its fonts load.
    if (element.firstElementChild) {
      observer.observe(element.firstElementChild)
    }
    return () => observer.disconnect()
  }, [])
  return (
    <div className="prose-no-margin relative my-6 overflow-auto" ref={container} tabIndex={scrolls ? 0 : undefined}>
      <table {...props} />
    </div>
  )
}
