import { createRootRoute, Outlet } from '@tanstack/react-router'
import { ThemeProvider } from '@/components/theme-provider'

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  return (
    <ThemeProvider>
      <div className="min-h-screen bg-background text-foreground">
        <Outlet />
      </div>
    </ThemeProvider>
  )
}
