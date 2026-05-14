interface TagChipsProps {
  tags: readonly string[] | undefined
}

export function TagChips({ tags }: TagChipsProps) {
  if (!tags || tags.length === 0) {
    return null
  }
  return (
    <div className="flex flex-wrap gap-1">
      {tags.map((tag) => (
        <span
          className="rounded-md border border-emerald-500/30 bg-emerald-500/5 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:text-emerald-300"
          key={tag}
        >
          {tag}
        </span>
      ))}
    </div>
  )
}
