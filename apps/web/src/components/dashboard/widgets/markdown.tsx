import { MarkdownContent } from '@/components/dashboard/markdown-content'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { MarkdownConfig } from '@/lib/widget-types'

interface MarkdownWidgetProps {
  config: MarkdownConfig
}

export function MarkdownWidget({ config }: MarkdownWidgetProps) {
  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <ScrollArea className="flex-1">
        <MarkdownContent
          className="prose prose-sm dark:prose-invert max-w-none text-sm [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h2]:mt-2 [&_h2]:mb-1 [&_h2]:font-semibold [&_h2]:text-base [&_h3]:mt-2 [&_h3]:mb-1 [&_h3]:font-semibold [&_h3]:text-sm [&_li]:my-0 [&_p]:my-1 [&_ul]:my-1 [&_ul]:pl-4"
          content={config.content || ''}
        />
      </ScrollArea>
    </div>
  )
}
