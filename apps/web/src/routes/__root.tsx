import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  return (
    <>
      {import.meta.env.DEV && <Agentation />}
      <ThemeProvider>
        <div className="min-h-screen bg-background text-foreground">
          <Outlet />
        </div>
        <Toaster />
      </ThemeProvider>
    </>
  )
}
