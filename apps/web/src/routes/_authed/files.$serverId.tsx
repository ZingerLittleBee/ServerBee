import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { ArrowLeft, FolderPlus, RefreshCw, Upload } from 'lucide-react'
import { useCallback, useReducer } from 'react'
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
import { CapabilityDisabledNotice } from '@/components/server/capability-disabled-notice'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useServer } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import type { FileEntry } from '@/hooks/use-file-api'
import { useFileDeleteMutation, useFileList, useStartDownloadMutation } from '@/hooks/use-file-api'
import { CAP_FILE, getEffectiveCapabilityEnabled } from '@/lib/capabilities'

function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback
}

export const Route = createFileRoute('/_authed/files/$serverId')({
  component: FilesPage,
  validateSearch: (search: Record<string, unknown>) => ({
    path: (typeof search.path === 'string' && search.path) || '/'
  })
})

interface ContextMenuState {
  entry: FileEntry
  position: { x: number; y: number }
}

interface FilesPageState {
  contextMenu: ContextMenuState | null
  deleteConfirm: FileEntry | null
  mkdirOpen: boolean
  renameEntry: FileEntry | null
  selectedFile: FileEntry | null
  uploadOpen: boolean
}

type FilesPageAction =
  | { type: 'confirmDelete'; entry: FileEntry }
  | { type: 'deleteFinished'; path: string }
  | { type: 'navigatePath' }
  | { type: 'renamedSelectedFile'; oldPath: string; newPath: string }
  | { type: 'setContextMenu'; value: ContextMenuState | null }
  | { type: 'setDeleteConfirm'; value: FileEntry | null }
  | { type: 'setMkdirOpen'; value: boolean }
  | { type: 'setRenameEntry'; value: FileEntry | null }
  | { type: 'setSelectedFile'; value: FileEntry | null }
  | { type: 'setUploadOpen'; value: boolean }

const INITIAL_FILES_PAGE_STATE: FilesPageState = {
  contextMenu: null,
  deleteConfirm: null,
  mkdirOpen: false,
  renameEntry: null,
  selectedFile: null,
  uploadOpen: false
}

function filesPageReducer(state: FilesPageState, action: FilesPageAction): FilesPageState {
  switch (action.type) {
    case 'confirmDelete':
      return { ...state, deleteConfirm: action.entry }
    case 'deleteFinished':
      return {
        ...state,
        deleteConfirm: null,
        selectedFile: state.selectedFile?.path === action.path ? null : state.selectedFile
      }
    case 'navigatePath':
      return { ...state, selectedFile: null }
    case 'renamedSelectedFile':
      if (state.selectedFile?.path !== action.oldPath) {
        return state
      }
      return {
        ...state,
        selectedFile: {
          ...state.selectedFile,
          name: action.newPath.split('/').pop() ?? state.selectedFile.name,
          path: action.newPath
        }
      }
    case 'setContextMenu':
      return { ...state, contextMenu: action.value }
    case 'setDeleteConfirm':
      return { ...state, deleteConfirm: action.value }
    case 'setMkdirOpen':
      return { ...state, mkdirOpen: action.value }
    case 'setRenameEntry':
      return { ...state, renameEntry: action.value }
    case 'setSelectedFile':
      return { ...state, selectedFile: action.value }
    case 'setUploadOpen':
      return { ...state, uploadOpen: action.value }
    default:
      return state
  }
}

function FilesPage() {
  const { t } = useTranslation('file')
  const { serverId } = Route.useParams()
  const { path: currentPath } = Route.useSearch()
  const navigate = useNavigate({ from: Route.fullPath })
  const queryClient = useQueryClient()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [state, dispatch] = useReducer(filesPageReducer, INITIAL_FILES_PAGE_STATE)

  const { data: server } = useServer(serverId)
  const fileDisabled =
    !!server && !getEffectiveCapabilityEnabled(server.effective_capabilities, server.capabilities, CAP_FILE)
  const { data: entries, isLoading, isError, error: listError } = useFileList(serverId, currentPath, !fileDisabled)
  const deleteMutation = useFileDeleteMutation(serverId)
  const downloadMutation = useStartDownloadMutation(serverId)

  const parentPath = currentPath === '/' ? null : currentPath.substring(0, currentPath.lastIndexOf('/')) || '/'

  const handleNavigate = useCallback(
    (path: string) => {
      navigate({ search: { path } })
      dispatch({ type: 'navigatePath' })
    },
    [navigate]
  )

  const handleFileSelect = useCallback((entry: FileEntry) => {
    dispatch({ type: 'setSelectedFile', value: entry })
  }, [])

  const handleContextMenu = useCallback((entry: FileEntry, event: React.MouseEvent) => {
    dispatch({ type: 'setContextMenu', value: { entry, position: { x: event.clientX, y: event.clientY } } })
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
          onError: (err) => toast.error(err instanceof Error ? err.message : t('download_failed'))
        }
      )
    },
    [downloadMutation, t]
  )

  const handleDelete = useCallback((entry: FileEntry) => {
    dispatch({ type: 'confirmDelete', entry })
  }, [])

  const confirmDelete = () => {
    if (!state.deleteConfirm) {
      return
    }
    const deletePath = state.deleteConfirm.path
    deleteMutation.mutate(
      { path: deletePath, recursive: state.deleteConfirm.file_type === 'Directory' },
      {
        onSuccess: () => {
          toast.success(t('delete'))
          dispatch({ type: 'deleteFinished', path: deletePath })
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('delete_failed'))
          dispatch({ type: 'setDeleteConfirm', value: null })
        }
      }
    )
  }

  const handleRename = useCallback((entry: FileEntry) => {
    dispatch({ type: 'setRenameEntry', value: entry })
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
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2 sm:gap-3 sm:px-4">
        <Link params={{ id: serverId }} search={{ range: 'realtime' }} to="/servers/$id">
          <Button size="sm" variant="ghost">
            <ArrowLeft aria-hidden="true" className="size-4" />
            {t('back_to_server')}
          </Button>
        </Link>
        <h1 className="font-semibold text-base sm:text-lg">{t('title')}</h1>
        <span className="text-muted-foreground text-xs sm:text-sm">{serverId.slice(0, 8)}...</span>
      </div>

      {fileDisabled ? (
        <CapabilityDisabledNotice />
      ) : (
        <>
          {/* Breadcrumb + Actions */}
          <div className="flex flex-col gap-2 border-b px-3 py-2 sm:flex-row sm:items-center sm:px-4 sm:py-1.5">
            <FileBreadcrumb onNavigate={handleNavigate} path={currentPath} />
            <div className="flex flex-wrap gap-1 sm:ml-auto">
              {isAdmin && (
                <Button
                  aria-label={t('upload')}
                  onClick={() => dispatch({ type: 'setUploadOpen', value: true })}
                  size="sm"
                  variant="outline"
                >
                  <Upload className="size-3.5" />
                  <span className="hidden sm:inline">{t('upload')}</span>
                </Button>
              )}
              {isAdmin && (
                <Button
                  aria-label={t('new_folder')}
                  onClick={() => dispatch({ type: 'setMkdirOpen', value: true })}
                  size="sm"
                  variant="outline"
                >
                  <FolderPlus className="size-3.5" />
                  <span className="hidden sm:inline">{t('new_folder')}</span>
                </Button>
              )}
              <Button
                aria-label={t('refresh')}
                onClick={handleRefresh}
                size="icon-sm"
                title={t('refresh')}
                variant="ghost"
              >
                <RefreshCw className="size-3.5" />
              </Button>
            </div>
          </div>

          {/* Main content: file list + preview */}
          <div className="flex min-h-0 flex-1">
            {/* File list panel */}
            <div className="w-full min-w-0 border-r md:w-[45%]">
              <FileBrowser
                entries={entries}
                error={isError ? getErrorMessage(listError, t('load_error')) : undefined}
                isLoading={isLoading}
                onContextMenu={handleContextMenu}
                onFileSelect={handleFileSelect}
                onNavigate={handleNavigate}
                parentPath={parentPath}
              />
            </div>

            {/* Preview/Editor panel - hidden on small screens */}
            <div className="hidden min-w-0 flex-1 md:block">
              <FilePreview entry={state.selectedFile} readOnly={!isAdmin} serverId={serverId} />
            </div>
          </div>
        </>
      )}

      {/* Mobile preview overlay */}
      {state.selectedFile && (
        <div className="fixed inset-0 z-40 flex flex-col bg-background md:hidden">
          <div className="flex min-w-0 items-center gap-2 border-b px-3 py-2 sm:px-4">
            <Button onClick={() => dispatch({ type: 'setSelectedFile', value: null })} size="sm" variant="ghost">
              <ArrowLeft aria-hidden="true" className="size-4" />
              {t('back_to_server')}
            </Button>
            <span className="truncate text-sm">{state.selectedFile.name}</span>
          </div>
          <div className="min-h-0 flex-1">
            <FilePreview entry={state.selectedFile} readOnly={!isAdmin} serverId={serverId} />
          </div>
        </div>
      )}

      {/* Transfer bar */}
      <TransferBar />

      {/* Context menu */}
      {state.contextMenu && (
        <FileContextMenu
          entry={state.contextMenu.entry}
          isAdmin={isAdmin}
          onClose={() => dispatch({ type: 'setContextMenu', value: null })}
          onCopyPath={handleCopyPath}
          onDelete={handleDelete}
          onDownload={handleDownload}
          onRename={handleRename}
          position={state.contextMenu.position}
        />
      )}

      {/* Dialogs */}
      <FileUploadDialog
        currentPath={currentPath}
        onClose={() => dispatch({ type: 'setUploadOpen', value: false })}
        open={state.uploadOpen}
        serverId={serverId}
      />
      <MkdirDialog
        currentPath={currentPath}
        onClose={() => dispatch({ type: 'setMkdirOpen', value: false })}
        open={state.mkdirOpen}
        serverId={serverId}
      />
      <RenameDialog
        entry={state.renameEntry}
        onClose={() => dispatch({ type: 'setRenameEntry', value: null })}
        onRenamed={(oldPath, newPath) => {
          dispatch({ type: 'renamedSelectedFile', oldPath, newPath })
        }}
        open={state.renameEntry !== null}
        serverId={serverId}
      />

      {/* Delete confirmation dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            dispatch({ type: 'setDeleteConfirm', value: null })
          }
        }}
        open={state.deleteConfirm !== null}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('confirm_delete_title')}</DialogTitle>
          </DialogHeader>
          <p className="text-muted-foreground text-sm">
            {t('confirm_delete', { name: state.deleteConfirm?.name ?? '' })}
          </p>
          <DialogFooter>
            <Button onClick={() => dispatch({ type: 'setDeleteConfirm', value: null })} variant="outline">
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
