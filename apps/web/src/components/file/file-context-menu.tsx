import { ClipboardCopy, Download, Pencil, Trash2 } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import type { FileEntry } from '@/hooks/use-file-api'

interface FileContextMenuProps {
  entry: FileEntry
  isAdmin?: boolean
  onClose: () => void
  onCopyPath: (entry: FileEntry) => void
  onDelete: (entry: FileEntry) => void
  onDownload: (entry: FileEntry) => void
  onRename: (entry: FileEntry) => void
  position: { x: number; y: number }
}

export function FileContextMenu({
  entry,
  isAdmin = true,
  position,
  onClose,
  onDownload,
  onDelete,
  onRename,
  onCopyPath
}: FileContextMenuProps) {
  const { t } = useTranslation('file')
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose()
      }
    }
    function handleEsc(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        onClose()
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    document.addEventListener('keydown', handleEsc)
    return () => {
      document.removeEventListener('mousedown', handleClickOutside)
      document.removeEventListener('keydown', handleEsc)
    }
  }, [onClose])

  const items = [
    {
      label: t('download'),
      icon: Download,
      action: () => onDownload(entry),
      show: entry.file_type !== 'Directory'
    },
    { label: t('rename'), icon: Pencil, action: () => onRename(entry), show: isAdmin },
    { label: t('copy_path'), icon: ClipboardCopy, action: () => onCopyPath(entry), show: true },
    { label: t('delete'), icon: Trash2, action: () => onDelete(entry), show: isAdmin, destructive: true }
  ]

  const handleMenuKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const menuItems = menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')
    if (!menuItems || menuItems.length === 0) {
      return
    }
    const currentIndex = Array.from(menuItems).indexOf(document.activeElement as HTMLButtonElement)
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const next = currentIndex < menuItems.length - 1 ? currentIndex + 1 : 0
      menuItems[next].focus()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      const prev = currentIndex > 0 ? currentIndex - 1 : menuItems.length - 1
      menuItems[prev].focus()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
    }
  }

  return (
    <div
      className="fixed z-50 min-w-[160px] rounded-lg border bg-popover p-1 shadow-md"
      onKeyDown={handleMenuKeyDown}
      ref={menuRef}
      role="menu"
      style={{ left: position.x, top: position.y }}
    >
      {items
        .filter((item) => item.show)
        .map((item) => (
          <button
            className={`flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors hover:bg-muted ${
              item.destructive ? 'text-destructive' : ''
            }`}
            key={item.label}
            onClick={() => {
              item.action()
              onClose()
            }}
            role="menuitem"
            type="button"
          >
            <item.icon aria-hidden="true" className="size-3.5" />
            {item.label}
          </button>
        ))}
    </div>
  )
}
