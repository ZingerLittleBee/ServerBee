import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_authed/files/$serverId')({
  component: FilesPage
})

function FilesPage() {
  const { serverId } = Route.useParams()
  return <div>Files for server {serverId} (coming soon)</div>
}
