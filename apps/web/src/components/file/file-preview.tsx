import { Download, FileText, Loader2, Save } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { FileEditor } from '@/components/file/file-editor'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { FileEntry } from '@/hooks/use-file-api'
import { useFileRead, useFileStat, useFileWriteMutation, useStartDownloadMutation } from '@/hooks/use-file-api'
import { extensionToLanguage, isImageFile, isTextFile } from '@/lib/file-utils'
import { formatBytes } from '@/lib/utils'

const MAX_PREVIEW_SIZE = 384 * 1024 // 384 KB — must stay within server's MAX_FILE_CHUNK_SIZE to avoid WS frame overflow

interface FilePreviewProps {
  entry: FileEntry | null
  serverId: string
}

export function FilePreview({ serverId, entry }: FilePreviewProps) {
  const { t } = useTranslation('file')

  if (!entry) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        <div className="text-center">
          <FileText className="mx-auto mb-2 size-10 opacity-30" />
          <p>{t('select_file')}</p>
        </div>
      </div>
    )
  }

  if (entry.file_type === 'Directory') {
    return null
  }

  const canPreviewText = isTextFile(entry.name) && entry.size < MAX_PREVIEW_SIZE
  const isImage = isImageFile(entry.name)

  if (canPreviewText) {
    return <TextPreview entry={entry} serverId={serverId} />
  }

  if (isImage || entry.size >= MAX_PREVIEW_SIZE) {
    return <FileInfoPanel entry={entry} serverId={serverId} />
  }

  return <FileInfoPanel entry={entry} serverId={serverId} />
}

function TextPreview({ serverId, entry }: { entry: FileEntry; serverId: string }) {
  const { t } = useTranslation('file')
  const { data: content, isLoading } = useFileRead(serverId, entry.path, true)
  const writeMutation = useFileWriteMutation(serverId)
  const editorRef = useRef<import('@/components/file/file-editor').MonacoEditorInstance | null>(null)
  const loadedModifiedRef = useRef<number>(entry.modified)
  const [conflictOpen, setConflictOpen] = useState(false)
  const pendingContentRef = useRef<string>('')

  useEffect(() => {
    loadedModifiedRef.current = entry.modified
  }, [entry.modified])

  const { refetch: refetchStat } = useFileStat(serverId, entry.path, false)

  const doSave = useCallback(
    (value: string) => {
      writeMutation.mutate(
        { path: entry.path, content: value },
        {
          onSuccess: () => {
            toast.success(t('save_success'))
          },
          onError: (err) => {
            toast.error(err instanceof Error ? err.message : 'Save failed')
          }
        }
      )
    },
    [writeMutation, entry.path, t]
  )

  const handleSave = useCallback(
    (value: string) => {
      pendingContentRef.current = value
      refetchStat().then(({ data: stat }) => {
        if (stat && stat.modified !== loadedModifiedRef.current) {
          setConflictOpen(true)
        } else {
          doSave(value)
        }
      })
    },
    [refetchStat, doSave]
  )

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const language = extensionToLanguage(entry.name)

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b px-3 py-1.5">
        <span className="truncate text-sm">{entry.name}</span>
        <Button
          disabled={writeMutation.isPending}
          onClick={() => {
            if (editorRef.current) {
              handleSave(editorRef.current.getValue())
            }
          }}
          size="sm"
          variant="outline"
        >
          <Save className="size-3.5" />
          {writeMutation.isPending ? t('saving') : t('save')}
        </Button>
      </div>
      <div className="flex-1">
        <FileEditor content={content ?? ''} editorRef={editorRef} language={language} onSave={handleSave} />
      </div>
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            setConflictOpen(false)
          }
        }}
        open={conflictOpen}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('save_conflict_title')}</DialogTitle>
          </DialogHeader>
          <p className="text-muted-foreground text-sm">{t('save_conflict_message')}</p>
          <DialogFooter>
            <Button onClick={() => setConflictOpen(false)} variant="outline">
              {t('cancel')}
            </Button>
            <Button
              onClick={() => {
                setConflictOpen(false)
                doSave(pendingContentRef.current)
              }}
            >
              {t('overwrite')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function FileInfoPanel({ serverId, entry }: { entry: FileEntry; serverId: string }) {
  const { t } = useTranslation('file')
  const downloadMutation = useStartDownloadMutation(serverId)

  const handleDownload = () => {
    downloadMutation.mutate(
      { path: entry.path },
      {
        onSuccess: () => {
          toast.success(t('download_started'))
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : 'Download failed')
        }
      }
    )
  }

  const isImage = isImageFile(entry.name)
  const isTooLarge = entry.size >= MAX_PREVIEW_SIZE

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-6">
      <FileText className="size-12 text-muted-foreground opacity-40" />
      <div className="space-y-1 text-center">
        <p className="font-medium">{entry.name}</p>
        <p className="text-muted-foreground text-sm">
          {formatBytes(entry.size)}
          {entry.permissions && ` | ${entry.permissions}`}
        </p>
        {isImage && <p className="text-muted-foreground text-xs">{t('type')}: Image</p>}
        {isTooLarge && <p className="text-muted-foreground text-xs">{t('file_too_large')}</p>}
      </div>
      <Button disabled={downloadMutation.isPending} onClick={handleDownload} size="sm" variant="outline">
        <Download className="size-3.5" />
        {t('download')}
      </Button>
    </div>
  )
}
