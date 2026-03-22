import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface SidebarState {
  open: boolean
  setOpen: (open: boolean) => void
  toggle: () => void
}

export const useSidebarStore = create<SidebarState>()(
  persist(
    (set) => ({
      open: true,
      setOpen: (open) => set({ open }),
      toggle: () => set((state) => ({ open: !state.open }))
    }),
    { name: 'sidebar-state' }
  )
)
