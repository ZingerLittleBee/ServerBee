import { ArrowUp, File, FileArchive, FileAudio, FileCode, FileImage, FileVideo, Folder, Link2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import type { FileEntry } from '@/hooks/use-file-api'
import { fileIcon } from '@/lib/file-utils'
import { formatBytes } from '@/lib/utils'

interface FileBrowserProps {
  entries: FileEntry[] | undefined
  error?: string
  isLoading?: boolean
  onContextMenu: (entry: FileEntry, event: React.MouseEvent) => void
  onFileSelect: (entry: FileEntry) => void
  onNavigate: (path: string) => void
  parentPath: string | null
}

const ICON_MAP: Record<string, React.ComponentType<{ className?: string }>> = {
  folder: Folder,
  'file-symlink': Link2,
  'file-image': FileImage,
  'file-text': File,
  'file-archive': FileArchive,
  'file-video': FileVideo,
  'file-audio': FileAudio,
  'file-code': FileCode,
  file: File
}

function EntryIcon({ entry }: { entry: FileEntry }) {
  const iconName = fileIcon(entry.file_type, entry.name)
  const Icon = ICON_MAP[iconName] ?? File

  if (entry.file_type === 'Directory') {
    return <Icon className="size-4 text-blue-500" />
  }
  if (entry.file_type === 'Symlink') {
    return <Icon className="size-4 text-purple-500" />
  }
  return <Icon className="size-4 text-muted-foreground" />
}

function formatModified(ts: number): string {
  if (ts <= 0) {
    return '-'
  }
  return new Date(ts * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  })
}

export function FileBrowser({
  entries,
  error,
  isLoading,
  parentPath,
  onNavigate,
  onFileSelect,
  onContextMenu
}: FileBrowserProps) {
  const { t } = useTranslation('file')

  if (isLoading) {
    return (
      <div className="space-y-2 p-3">
        {Array.from({ length: 6 }, (_, i) => (
          <Skeleton className="h-8" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex h-32 items-center justify-center p-4">
        <p className="text-destructive text-sm">{error}</p>
      </div>
    )
  }

  const isEmpty = !entries || entries.length === 0

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-[50%]">{t('name')}</TableHead>
          <TableHead className="w-[20%]">{t('size')}</TableHead>
          <TableHead className="w-[30%]">{t('modified')}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {parentPath !== null && (
          <TableRow className="cursor-pointer" onClick={() => onNavigate(parentPath)}>
            <TableCell className="flex items-center gap-2">
              <ArrowUp className="size-4 text-muted-foreground" />
              <span>..</span>
            </TableCell>
            <TableCell />
            <TableCell />
          </TableRow>
        )}
        {isEmpty && (
          <TableRow>
            <TableCell className="text-center text-muted-foreground" colSpan={3}>
              {t('empty_directory')}
            </TableCell>
          </TableRow>
        )}
        {entries?.map((entry) => (
          <TableRow
            className="cursor-pointer"
            key={entry.path}
            onClick={() => {
              if (entry.file_type === 'Directory') {
                onNavigate(entry.path)
              } else {
                onFileSelect(entry)
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault()
              onContextMenu(entry, e)
            }}
          >
            <TableCell>
              <div className="flex items-center gap-2">
                <EntryIcon entry={entry} />
                <span className="truncate">{entry.name}</span>
              </div>
            </TableCell>
            <TableCell className="text-muted-foreground text-xs">
              {entry.file_type === 'Directory' ? '-' : formatBytes(entry.size)}
            </TableCell>
            <TableCell className="text-muted-foreground text-xs">{formatModified(entry.modified)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}
