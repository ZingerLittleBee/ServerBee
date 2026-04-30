import { createFileRoute } from '@tanstack/react-router'
import { ThemeEditor } from '@/components/theme/theme-editor'

export const Route = createFileRoute('/_authed/settings/appearance/themes/$id')({
  component: EditThemePage
})

function EditThemePage() {
  const { id } = Route.useParams()
  return <ThemeEditor themeId={Number(id)} />
}
