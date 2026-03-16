import { Loader2, Upload } from 'lucide-react'
import { type DragEvent, useCallback, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useUploadFileMutation } from '@/hooks/use-file-api'
import { formatBytes } from '@/lib/utils'

interface FileUploadDialogProps {
  currentPath: string
  onClose: () => void
  open: boolean
  serverId: string
}

export function FileUploadDialog({ serverId, currentPath, open, onClose }: FileUploadDialogProps) {
  const { t } = useTranslation('file')
  const [selectedFile, setSelectedFile] = useState<File | null>(null)
  const [dragActive, setDragActive] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const uploadMutation = useUploadFileMutation(serverId)

  const handleDrop = useCallback((e: DragEvent) => {
    e.preventDefault()
    setDragActive(false)
    const file = e.dataTransfer.files[0]
    if (file) {
      setSelectedFile(file)
    }
  }, [])

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault()
    setDragActive(true)
  }, [])

  const handleDragLeave = useCallback(() => {
    setDragActive(false)
  }, [])

  const handleFileChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (file) {
      setSelectedFile(file)
    }
  }, [])

  const handleUpload = () => {
    if (!selectedFile) {
      return
    }
    const dest = currentPath.endsWith('/')
      ? `${currentPath}${selectedFile.name}`
      : `${currentPath}/${selectedFile.name}`
    uploadMutation.mutate(
      { path: dest, file: selectedFile },
      {
        onSuccess: () => {
          toast.success(t('upload_success'))
          setSelectedFile(null)
          onClose()
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : 'Upload failed')
        }
      }
    )
  }

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setSelectedFile(null)
      onClose()
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('upload')}</DialogTitle>
        </DialogHeader>

        <button
          className={`flex min-h-32 cursor-pointer flex-col items-center justify-center rounded-lg border-2 border-dashed p-6 transition-colors ${
            dragActive ? 'border-primary bg-primary/5' : 'border-border'
          }`}
          onClick={() => inputRef.current?.click()}
          onDragLeave={handleDragLeave}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
          type="button"
        >
          <Upload className="mb-2 size-8 text-muted-foreground" />
          <p className="text-center text-muted-foreground text-sm">{t('drop_files')}</p>
          {selectedFile && (
            <p className="mt-2 text-center text-sm">
              {selectedFile.name} ({formatBytes(selectedFile.size)})
            </p>
          )}
        </button>

        <input className="hidden" onChange={handleFileChange} ref={inputRef} type="file" />

        <p className="text-muted-foreground text-xs">
          {t('remote_path')}: {currentPath}
        </p>

        <DialogFooter>
          <Button onClick={() => handleOpenChange(false)} variant="outline">
            {t('cancel')}
          </Button>
          <Button disabled={!selectedFile || uploadMutation.isPending} onClick={handleUpload}>
            {uploadMutation.isPending ? (
              <>
                <Loader2 className="size-3.5 animate-spin" />
                {t('uploading')}
              </>
            ) : (
              t('upload')
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
