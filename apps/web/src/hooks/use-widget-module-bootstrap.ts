import { useEffect, useRef } from 'react'
import { bootstrapLoader } from '@/widgets-runtime/loader'

export function useWidgetModuleBootstrap(enabled: boolean) {
  const bootstrapped = useRef(false)

  useEffect(() => {
    if (!enabled || bootstrapped.current) {
      return
    }

    let active = true
    bootstrapped.current = true

    bootstrapLoader().catch((err) => {
      if (!active) {
        return
      }
      bootstrapped.current = false
      console.warn('widget bootstrap failed', err)
    })

    return () => {
      active = false
    }
  }, [enabled])
}
