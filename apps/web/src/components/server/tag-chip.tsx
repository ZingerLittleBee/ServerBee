import { cn } from '@/lib/utils'

const PALETTE = [
  'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400',
  'bg-sky-500/15 text-sky-700 dark:text-sky-400',
  'bg-amber-500/15 text-amber-700 dark:text-amber-400',
  'bg-rose-500/15 text-rose-700 dark:text-rose-400',
  'bg-violet-500/15 text-violet-700 dark:text-violet-400',
  'bg-slate-500/15 text-slate-700 dark:text-slate-300'
] as const

function hashTag(tag: string): number {
  let h = 0
  for (let i = 0; i < tag.length; i++) {
    h = (h * 31 + tag.charCodeAt(i)) | 0
  }
  return Math.abs(h) % PALETTE.length
}

interface TagChipRowProps {
  className?: string
  tags: string[] | undefined
}

export function TagChipRow({ tags, className }: TagChipRowProps) {
  if (!tags || tags.length === 0) {
    return null
  }
  return (
    <div className={cn('mt-1 flex flex-wrap gap-1', className)}>
      {tags.map((tag) => (
        <span
          className={cn(
            'inline-flex max-w-[80px] items-center truncate rounded px-1.5 py-0.5 text-[10px] font-medium leading-tight',
            PALETTE[hashTag(tag)]
          )}
          data-slot="tag-chip"
          key={tag}
          title={tag}
        >
          {tag}
        </span>
      ))}
    </div>
  )
}
