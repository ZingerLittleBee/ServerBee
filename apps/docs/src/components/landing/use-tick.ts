import { useEffect, useState, useSyncExternalStore } from 'react'

/** How often the live mocks advance. */
const TICK_MS = 1800

const REDUCED_MOTION = '(prefers-reduced-motion: reduce)'

function subscribeReducedMotion(onChange: () => void): () => void {
  const query = window.matchMedia(REDUCED_MOTION)
  query.addEventListener('change', onChange)
  return () => query.removeEventListener('change', onChange)
}

/**
 * Whether the visitor asks for reduced motion. False on the server and on the first client render, so
 * hydration matches; the real value follows right after hydration and on every change.
 */
export function useReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribeReducedMotion,
    () => window.matchMedia(REDUCED_MOTION).matches,
    () => false
  )
}

/**
 * The landing's shared clock. Returns 0 on the server and on the first client render, so hydration
 * matches, then counts up every 1.8 s while `running` is true and the tab is visible. A stopped clock
 * keeps its value, so the live mocks hold their current frame.
 */
export function useTick(running: boolean): number {
  const [tick, setTick] = useState(0)

  useEffect(() => {
    if (!running) {
      return
    }
    let timer: number | undefined

    const stop = () => {
      window.clearInterval(timer)
      timer = undefined
    }
    const sync = () => {
      if (document.hidden) {
        stop()
      } else if (timer === undefined) {
        timer = window.setInterval(() => setTick((current) => current + 1), TICK_MS)
      }
    }

    sync()
    document.addEventListener('visibilitychange', sync)
    return () => {
      stop()
      document.removeEventListener('visibilitychange', sync)
    }
  }, [running])

  return tick
}
