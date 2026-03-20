import { useMemo } from 'react'
import { renderMarkdown } from '@/lib/markdown'
import type { MarkdownConfig } from '@/lib/widget-types'

interface MarkdownWidgetProps {
  config: MarkdownConfig
}

export function MarkdownWidget({ config }: MarkdownWidgetProps) {
  const html = useMemo(() => renderMarkdown(config.content || ''), [config.content])

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div
        className="prose prose-sm dark:prose-invert max-w-none flex-1 overflow-auto text-sm [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h2]:mt-2 [&_h2]:mb-1 [&_h2]:font-semibold [&_h2]:text-base [&_h3]:mt-2 [&_h3]:mb-1 [&_h3]:font-semibold [&_h3]:text-sm [&_li]:my-0 [&_p]:my-1 [&_ul]:my-1 [&_ul]:pl-4"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: renderMarkdown escapes all raw HTML and validates URLs
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  )
}
