import { Upload } from 'lucide-react'
import { type ChangeEvent, type DragEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { type ApiError, type UploadResult, useUploadSpaTheme } from '@/api/spa-themes'

export function SpaThemeUploadCard() {
  const { t } = useTranslation('spa-theme')
  const upload = useUploadSpaTheme()
  const inputRef = useRef<HTMLInputElement>(null)
  const [dragOver, setDragOver] = useState(false)

  function pick() {
    inputRef.current?.click()
  }

  function submit(file: File) {
    upload.mutate(file, {
      onError: (err: ApiError) => {
        const code = err.code ?? 'default'
        const details = (err.details ?? {}) as Record<string, unknown>
        toast.error(t(`errors.${code}` as never, { ...details, message: err.message }))
      },
      onSuccess: (result: UploadResult) => {
        if (result.is_upgrade_of) {
          toast.success(t('upload_upgrade_success', { previous_version: result.is_upgrade_of.previous_version }))
        } else {
          toast.success(t('upload_success'))
        }
      }
    })
  }

  function onChange(e: ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0]
    if (f) {
      submit(f)
    }
  }

  function onDrop(e: DragEvent<HTMLButtonElement>) {
    e.preventDefault()
    setDragOver(false)
    const f = e.dataTransfer.files?.[0]
    if (f) {
      submit(f)
    }
  }

  return (
    <button
      className={`flex aspect-video w-full cursor-pointer flex-col items-center justify-center rounded-lg border-2 border-dashed bg-transparent text-left transition ${
        dragOver ? 'border-primary bg-accent/40' : 'border-muted'
      }`}
      onClick={pick}
      onDragLeave={() => setDragOver(false)}
      onDragOver={(e) => {
        e.preventDefault()
        setDragOver(true)
      }}
      onDrop={onDrop}
      type="button"
    >
      <input accept=".sbtheme,.zip,application/zip" className="hidden" onChange={onChange} ref={inputRef} type="file" />
      <Upload className="mb-2 size-6 text-muted-foreground" />
      <div className="text-muted-foreground text-sm">{upload.isPending ? t('upload_progress') : t('upload_drag')}</div>
    </button>
  )
}
