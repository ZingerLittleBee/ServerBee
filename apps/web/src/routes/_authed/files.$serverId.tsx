import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, FolderPlus, RefreshCw, Upload } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { FileBreadcrumb } from '@/components/file/file-breadcrumb'
import { FileBrowser } from '@/components/file/file-browser'
import { FileContextMenu } from '@/components/file/file-context-menu'
import { FilePreview } from '@/components/file/file-preview'
import { FileUploadDialog } from '@/components/file/file-upload-dialog'
import { MkdirDialog } from '@/components/file/mkdir-dialog'
import { RenameDialog } from '@/components/file/rename-dialog'
import { TransferBar } from '@/components/file/transfer-bar'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { FileEntry } from '@/hooks/use-file-api'
import { useFileDeleteMutation, useFileList, useStartDownloadMutation } from '@/hooks/use-file-api'

export const Route = createFileRoute('/_authed/files/$serverId')({
  component: FilesPage
})

interface ContextMenuState {
  entry: FileEntry
  position: { x: number; y: number }
}

function FilesPage() {
  const { t } = useTranslation('file')
  const { serverId } = Route.useParams()
  const queryClient = useQueryClient()

  const [currentPath, setCurrentPath] = useState('/')
  const [selectedFile, setSelectedFile] = useState<FileEntry | null>(null)
  const [uploadOpen, setUploadOpen] = useState(false)
  const [mkdirOpen, setMkdirOpen] = useState(false)
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  const [deleteConfirm, setDeleteConfirm] = useState<FileEntry | null>(null)
  const [renameEntry, setRenameEntry] = useState<FileEntry | null>(null)

  const { data: entries, isLoading } = useFileList(serverId, currentPath)
  const deleteMutation = useFileDeleteMutation(serverId)
  const downloadMutation = useStartDownloadMutation(serverId)

  const parentPath = currentPath === '/' ? null : currentPath.substring(0, currentPath.lastIndexOf('/')) || '/'

  const handleNavigate = useCallback((path: string) => {
    setCurrentPath(path)
    setSelectedFile(null)
  }, [])

  const handleFileSelect = useCallback((entry: FileEntry) => {
    setSelectedFile(entry)
  }, [])

  const handleContextMenu = useCallback((entry: FileEntry, event: React.MouseEvent) => {
    setContextMenu({
      entry,
      position: { x: event.clientX, y: event.clientY }
    })
  }, [])

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: ['files', serverId, 'list', currentPath] })
  }

  const handleDownload = useCallback(
    (entry: FileEntry) => {
      downloadMutation.mutate(
        { path: entry.path },
        {
          onSuccess: () => toast.success(t('download_started')),
          onError: (err) => toast.error(err instanceof Error ? err.message : 'Download failed')
        }
      )
    },
    [downloadMutation, t]
  )

  const handleDelete = useCallback((entry: FileEntry) => {
    setDeleteConfirm(entry)
  }, [])

  const confirmDelete = () => {
    if (!deleteConfirm) {
      return
    }
    deleteMutation.mutate(
      { path: deleteConfirm.path, recursive: deleteConfirm.file_type === 'Directory' },
      {
        onSuccess: () => {
          toast.success(t('delete'))
          if (selectedFile?.path === deleteConfirm.path) {
            setSelectedFile(null)
          }
          setDeleteConfirm(null)
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : 'Delete failed')
          setDeleteConfirm(null)
        }
      }
    )
  }

  const handleRename = useCallback((entry: FileEntry) => {
    setRenameEntry(entry)
  }, [])

  const handleCopyPath = useCallback(
    (entry: FileEntry) => {
      navigator.clipboard.writeText(entry.path).then(() => {
        toast.success(t('copy_path'))
      })
    },
    [t]
  )

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-3 border-b px-4 py-2">
        <Link params={{ id: serverId }} to="/servers/$id">
          <Button size="sm" variant="ghost">
            <ArrowLeft className="size-4" />
            {t('back_to_server')}
          </Button>
        </Link>
        <h1 className="font-semibold text-lg">{t('title')}</h1>
        <span className="text-muted-foreground text-sm">{serverId.slice(0, 8)}...</span>
      </div>

      {/* Breadcrumb + Actions */}
      <div className="flex items-center gap-2 border-b px-4 py-1.5">
        <FileBreadcrumb onNavigate={handleNavigate} path={currentPath} />
        <div className="ml-auto flex gap-1">
          <Button onClick={() => setUploadOpen(true)} size="sm" variant="outline">
            <Upload className="size-3.5" />
            <span className="hidden sm:inline">{t('upload')}</span>
          </Button>
          <Button onClick={() => setMkdirOpen(true)} size="sm" variant="outline">
            <FolderPlus className="size-3.5" />
            <span className="hidden sm:inline">{t('new_folder')}</span>
          </Button>
          <Button onClick={handleRefresh} size="icon-sm" title={t('refresh')} variant="ghost">
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
      </div>

      {/* Main content: file list + preview */}
      <div className="flex min-h-0 flex-1">
        {/* File list panel */}
        <div className="w-full min-w-0 overflow-y-auto border-r md:w-[45%]">
          <FileBrowser
            entries={entries}
            isLoading={isLoading}
            onContextMenu={handleContextMenu}
            onFileSelect={handleFileSelect}
            onNavigate={handleNavigate}
            parentPath={parentPath}
          />
        </div>

        {/* Preview/Editor panel - hidden on small screens */}
        <div className="hidden min-w-0 flex-1 md:block">
          <FilePreview entry={selectedFile} serverId={serverId} />
        </div>
      </div>

      {/* Mobile preview overlay */}
      {selectedFile && (
        <div className="fixed inset-0 z-40 flex flex-col bg-background md:hidden">
          <div className="flex items-center gap-2 border-b px-4 py-2">
            <Button onClick={() => setSelectedFile(null)} size="sm" variant="ghost">
              <ArrowLeft className="size-4" />
              {t('back_to_server')}
            </Button>
            <span className="truncate text-sm">{selectedFile.name}</span>
          </div>
          <div className="flex-1">
            <FilePreview entry={selectedFile} serverId={serverId} />
          </div>
        </div>
      )}

      {/* Transfer bar */}
      <TransferBar />

      {/* Context menu */}
      {contextMenu && (
        <FileContextMenu
          entry={contextMenu.entry}
          onClose={() => setContextMenu(null)}
          onCopyPath={handleCopyPath}
          onDelete={handleDelete}
          onDownload={handleDownload}
          onRename={handleRename}
          position={contextMenu.position}
        />
      )}

      {/* Dialogs */}
      <FileUploadDialog
        currentPath={currentPath}
        onClose={() => setUploadOpen(false)}
        open={uploadOpen}
        serverId={serverId}
      />
      <MkdirDialog currentPath={currentPath} onClose={() => setMkdirOpen(false)} open={mkdirOpen} serverId={serverId} />
      <RenameDialog
        entry={renameEntry}
        onClose={() => setRenameEntry(null)}
        open={renameEntry !== null}
        serverId={serverId}
      />

      {/* Delete confirmation dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            setDeleteConfirm(null)
          }
        }}
        open={deleteConfirm !== null}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('confirm_delete_title')}</DialogTitle>
          </DialogHeader>
          <p className="text-muted-foreground text-sm">{t('confirm_delete', { name: deleteConfirm?.name ?? '' })}</p>
          <DialogFooter>
            <Button onClick={() => setDeleteConfirm(null)} variant="outline">
              {t('cancel')}
            </Button>
            <Button disabled={deleteMutation.isPending} onClick={confirmDelete} variant="destructive">
              {t('delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
