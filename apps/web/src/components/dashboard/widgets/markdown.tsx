import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { MarkdownContent } from '@/components/dashboard/markdown-content'
import { WIDGET_PADDING_CLASS_NAME } from '@/components/dashboard/widget-chrome'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import type { MarkdownConfig } from '@/lib/widget-types'

interface MarkdownWidgetProps {
  config: MarkdownConfig
}

export function MarkdownWidget({ config }: MarkdownWidgetProps) {
  return (
    <DashboardCard className={cn('flex h-full flex-col bg-card', WIDGET_PADDING_CLASS_NAME)}>
      <ScrollArea className="flex-1">
        <MarkdownContent
          className="max-w-none text-sm [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:mt-2 [&_h1]:mb-1 [&_h1]:font-semibold [&_h1]:text-lg [&_h2]:mt-2 [&_h2]:mb-1 [&_h2]:font-semibold [&_h2]:text-base [&_h3]:mt-2 [&_h3]:mb-1 [&_h3]:font-semibold [&_h3]:text-sm [&_li]:my-0 [&_p]:my-1 [&_ul]:my-1 [&_ul]:list-disc [&_ul]:pl-4"
          content={config.content || ''}
        />
      </ScrollArea>
    </DashboardCard>
  )
}
