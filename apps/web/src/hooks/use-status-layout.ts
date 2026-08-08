import { useCallback, useEffect, useState } from 'react'

export const STATUS_LAYOUT_STORAGE_KEY = 'serverbee.status.layout'

export type StatusLayout = 'grid' | 'list'

function isStatusLayout(value: string | null): value is StatusLayout {
  return value === 'grid' || value === 'list'
}

/** Persisted layout choice; null when unset, invalid, or storage is unavailable. */
export function readStoredStatusLayout(): StatusLayout | null {
  try {
    const stored = localStorage.getItem(STATUS_LAYOUT_STORAGE_KEY)
    return isStatusLayout(stored) ? stored : null
  } catch {
    // localStorage may be unavailable (private mode / disabled storage)
    return null
  }
}

/**
 * Effective status-overview layout: the persisted choice wins, then the
 * status-page config default, then 'grid'.
 *
 * The initial state reads storage synchronously so the very first render —
 * including the loading skeleton — already matches the layout the loaded
 * page will render (no skeleton → content layout jump). The effect then
 * reconciles once the async config default arrives.
 */
export function useStatusLayout(defaultLayout: StatusLayout | null | undefined) {
  const [layout, setLayout] = useState<StatusLayout>(() => readStoredStatusLayout() ?? defaultLayout ?? 'grid')

  useEffect(() => {
    setLayout(readStoredStatusLayout() ?? defaultLayout ?? 'grid')
  }, [defaultLayout])

  const onLayoutChange = useCallback((next: StatusLayout) => {
    setLayout(next)
    try {
      localStorage.setItem(STATUS_LAYOUT_STORAGE_KEY, next)
    } catch {
      // ignore storage failures (private mode / quota)
    }
  }, [])

  return [layout, onLayoutChange] as const
}
