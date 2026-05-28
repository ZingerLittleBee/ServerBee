import { createFileRoute } from '@tanstack/react-router'
import { Download, Loader2, Trash2, Upload } from 'lucide-react'
import { type ChangeEvent, type FormEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  type ModuleSummary,
  useInstallFromFile,
  useInstallFromUrl,
  useUninstallWidgetModule,
  useWidgetModules
} from '@/api/widget-modules'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'

export const Route = createFileRoute('/_authed/settings/widgets')({
  component: WidgetsPage
})

function manifestDisplayName(m: ModuleSummary): string {
  const manifest = m.manifest as { name?: unknown }
  if (typeof manifest.name === 'string' && manifest.name.length > 0) {
    return manifest.name
  }
  return m.id
}

function WidgetsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const list = useWidgetModules()
  const installUrl = useInstallFromUrl()
  const installFile = useInstallFromFile()
  const uninstall = useUninstallWidgetModule()

  const [url, setUrl] = useState('')
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const handleInstallUrl = (e: FormEvent) => {
    e.preventDefault()
    const trimmed = url.trim()
    if (trimmed.length === 0) {
      return
    }
    installUrl.mutate(trimmed, {
      onSuccess: (data) => {
        setUrl('')
        toast.success(t('widgets.toast_installed', { id: data.id, version: data.version }))
      },
      onError: (err) => {
        toast.error(err.message)
      }
    })
  }

  const handleInstallFile = (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) {
      return
    }
    installFile.mutate(file, {
      onSuccess: (data) => {
        toast.success(t('widgets.toast_installed', { id: data.id, version: data.version }))
      },
      onError: (err) => {
        toast.error(err.message)
      },
      onSettled: () => {
        if (fileInputRef.current) {
          fileInputRef.current.value = ''
        }
      }
    })
  }

  const handleUninstall = (id: string) => {
    uninstall.mutate(id, {
      onSuccess: () => {
        toast.success(t('widgets.toast_uninstalled', { id }))
      },
      onError: (err) => {
        toast.error(err.message)
      }
    })
    setDeleteId(null)
  }

  return (
    <div>
      <div className="max-w-3xl space-y-6">
        <div className="space-y-3">
          <h2 className="font-semibold text-lg">{t('widgets.install_from_url')}</h2>
          <p className="text-muted-foreground text-sm">{t('widgets.install_from_url_desc')}</p>
          <form className="flex gap-2" onSubmit={handleInstallUrl}>
            <Input
              aria-label={t('widgets.url_label')}
              autoComplete="off"
              className="flex-1"
              disabled={installUrl.isPending}
              name="widget-url"
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/foo.widget.js"
              type="url"
              value={url}
            />
            <Button disabled={installUrl.isPending || url.trim().length === 0} type="submit">
              {installUrl.isPending ? (
                <Loader2 aria-hidden="true" className="size-4 animate-spin" />
              ) : (
                <Download aria-hidden="true" className="size-4" />
              )}
              {t('widgets.install')}
            </Button>
          </form>
        </div>

        <div className="space-y-3">
          <h2 className="font-semibold text-lg">{t('widgets.upload_file')}</h2>
          <p className="text-muted-foreground text-sm">{t('widgets.upload_file_desc')}</p>
          <div className="flex items-center gap-2">
            <Input
              accept=".js,.mjs"
              aria-label={t('widgets.upload_file')}
              className="flex-1"
              disabled={installFile.isPending}
              name="widget-file"
              onChange={handleInstallFile}
              ref={fileInputRef}
              type="file"
            />
            {installFile.isPending && (
              <span className="flex items-center gap-1 text-muted-foreground text-sm">
                <Loader2 aria-hidden="true" className="size-4 animate-spin" />
                {t('widgets.uploading')}
              </span>
            )}
            {!installFile.isPending && <Upload aria-hidden="true" className="size-4 text-muted-foreground" />}
          </div>
        </div>

        <div className="space-y-3">
          <h2 className="font-semibold text-lg">{t('widgets.installed_modules')}</h2>

          {list.isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 3 }, (_, i) => (
                <Skeleton className="h-14" key={`skeleton-${i.toString()}`} />
              ))}
            </div>
          )}

          {list.error && (
            <p className="text-destructive text-sm">
              {list.error instanceof Error ? list.error.message : t('common:errors.operation_failed')}
            </p>
          )}

          {!(list.isLoading || list.error) && (!list.data || list.data.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">{t('widgets.no_modules')}</p>
          )}

          {!list.isLoading && list.data && list.data.length > 0 && (
            <div className="divide-y rounded-md border">
              {list.data.map((m) => {
                const isBuiltin = m.source_type === 'Builtin'
                const displayName = manifestDisplayName(m)
                return (
                  <div
                    className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
                    key={m.id}
                  >
                    <div className="min-w-0">
                      <p className="truncate font-medium text-sm">{displayName}</p>
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-muted-foreground text-xs">
                        <span className="font-mono">{m.id}</span>
                        <span>v{m.version}</span>
                        <span>{m.source_type}</span>
                      </div>
                    </div>
                    {isBuiltin ? (
                      <span className="text-muted-foreground text-xs sm:text-right">{t('widgets.builtin_locked')}</span>
                    ) : (
                      <AlertDialog
                        onOpenChange={(open) => {
                          if (!open) {
                            setDeleteId(null)
                          }
                        }}
                        open={deleteId === m.id}
                      >
                        <AlertDialogTrigger
                          onClick={() => setDeleteId(m.id)}
                          render={
                            <Button
                              aria-label={`${t('widgets.uninstall')} ${displayName}`}
                              disabled={uninstall.isPending}
                              size="sm"
                              variant="destructive"
                            />
                          }
                        >
                          <Trash2 aria-hidden="true" className="size-3.5" />
                          {t('widgets.uninstall')}
                        </AlertDialogTrigger>
                        <AlertDialogContent>
                          <AlertDialogHeader>
                            <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                            <AlertDialogDescription>
                              {t('widgets.confirm_uninstall', { name: displayName })}
                            </AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                            <AlertDialogAction onClick={() => handleUninstall(m.id)} variant="destructive">
                              {t('widgets.uninstall')}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
