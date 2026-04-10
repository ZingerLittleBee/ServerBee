import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { DevProxyBanner } from '@/components/dev-proxy-banner'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  return (
    <>
      {import.meta.env.DEV && <Agentation />}
      <DevProxyBanner />
      <ThemeProvider>
        <TooltipProvider>
          <div className="h-screen overflow-hidden bg-background text-foreground">
            <Outlet />
          </div>
          <Toaster />
        </TooltipProvider>
      </ThemeProvider>
    </>
  )
}
