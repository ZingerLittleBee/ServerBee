import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useCreateTheme } from '@/api/themes'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { type ColorTheme, themes } from '@/themes'
import { presetVars } from '@/themes/preset-vars'

export const Route = createFileRoute('/_authed/settings/appearance/themes/new')({
  component: NewThemePage
})

export function NewThemePage() {
  const { t } = useTranslation(['settings', 'common'])
  const navigate = Route.useNavigate()
  const createTheme = useCreateTheme()
  const [name, setName] = useState('')
  const [forkFrom, setForkFrom] = useState<ColorTheme>('default')

  const submit = () => {
    const source = presetVars[forkFrom]
    createTheme.mutate(
      {
        based_on: forkFrom,
        name: name.trim() || t('appearance.editor.untitled'),
        vars_dark: source.dark,
        vars_light: source.light
      },
      {
        onSuccess: (created) => {
          navigate({
            params: { id: String(created.id) },
            to: '/settings/appearance/themes/$id'
          })
        }
      }
    )
  }

  return (
    <div className="max-w-md space-y-4 p-6">
      <h1 className="font-bold text-2xl">{t('appearance.editor.new_title')}</h1>
      <Input
        onChange={(event) => setName(event.target.value)}
        placeholder={t('appearance.editor.name_placeholder')}
        value={name}
      />
      <Select
        onValueChange={(value) => {
          if (value !== null) {
            setForkFrom(value as ColorTheme)
          }
        }}
        value={forkFrom}
      >
        <SelectTrigger>
          <SelectValue placeholder={t('appearance.editor.fork_from')} />
        </SelectTrigger>
        <SelectContent>
          {themes.map((theme) => (
            <SelectItem key={theme.id} value={theme.id}>
              {theme.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className="flex gap-2">
        <Button onClick={() => navigate({ to: '/settings/appearance' })} type="button" variant="outline">
          {t('common:cancel')}
        </Button>
        <Button disabled={createTheme.isPending} onClick={submit} type="button">
          {t('common:create')}
        </Button>
      </div>
    </div>
  )
}
