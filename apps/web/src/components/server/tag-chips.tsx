interface TagChipsProps {
  tags: readonly string[] | undefined
}

export function TagChips({ tags }: TagChipsProps) {
  if (!tags || tags.length === 0) {
    return (
      <div aria-hidden="true" className="flex flex-wrap gap-1">
        <span className="invisible rounded-sm border px-1.5 py-0.5 text-[10px] leading-[1.2]">&nbsp;</span>
      </div>
    )
  }
  return (
    <div className="flex flex-wrap gap-1">
      {tags.map((tag) => (
        <span
          className="rounded-sm border border-emerald-500/30 bg-emerald-500/5 px-1.5 py-0.5 text-[10px] text-emerald-700 leading-[1.2] dark:text-emerald-300"
          key={tag}
        >
          {tag}
        </span>
      ))}
    </div>
  )
}
