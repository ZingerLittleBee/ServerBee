import { createFileRoute, Outlet } from '@tanstack/react-router'

export const Route = createFileRoute('/status')({
  component: StatusLayout
})

function StatusLayout() {
  return <Outlet />
}
