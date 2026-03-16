import { ChevronRight, Home } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

interface FileBreadcrumbProps {
  onNavigate: (path: string) => void
  path: string
}

export function FileBreadcrumb({ path, onNavigate }: FileBreadcrumbProps) {
  const { t } = useTranslation('file')

  const segments = path.split('/').filter(Boolean)
  const paths: { label: string; path: string }[] = []

  for (let i = 0; i < segments.length; i++) {
    paths.push({
      label: segments[i],
      path: `/${segments.slice(0, i + 1).join('/')}`
    })
  }

  return (
    <div className="flex items-center gap-0.5 overflow-x-auto text-sm">
      <Button
        className="shrink-0 gap-1"
        onClick={() => onNavigate('/')}
        size="sm"
        title={t('breadcrumb_root')}
        variant="ghost"
      >
        <Home className="size-3.5" />
        <span className="hidden sm:inline">/</span>
      </Button>
      {paths.map((seg, idx) => (
        <span className="flex shrink-0 items-center gap-0.5" key={seg.path}>
          <ChevronRight className="size-3 text-muted-foreground" />
          {idx === paths.length - 1 ? (
            <span className="px-1.5 py-0.5 font-medium">{seg.label}</span>
          ) : (
            <Button className="shrink-0" onClick={() => onNavigate(seg.path)} size="sm" variant="ghost">
              {seg.label}
            </Button>
          )}
        </span>
      ))}
    </div>
  )
}
