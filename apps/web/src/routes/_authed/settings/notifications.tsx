import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { api } from '@/lib/api-client'
import type { Notification, NotificationGroup } from '@/lib/api-schema'
import {
  EmailFormFields as EmailFormFieldsImpl,
  type EmailFormFieldsProps,
  NotificationChannelsSection
} from './notification-channel-section'
import { NotificationGroupsSection } from './notification-groups-section'

export const Route = createFileRoute('/_authed/settings/notifications')({
  component: NotificationsPage
})

export function EmailFormFields(props: EmailFormFieldsProps) {
  return <EmailFormFieldsImpl {...props} />
}

function NotificationsPage() {
  const { data: notifications, isLoading } = useQuery<Notification[]>({
    queryKey: ['notifications'],
    queryFn: () => api.get<Notification[]>('/api/notifications')
  })
  const { data: groups } = useQuery<NotificationGroup[]>({
    queryKey: ['notification-groups'],
    queryFn: () => api.get<NotificationGroup[]>('/api/notification-groups')
  })

  return (
    <div>
      <div className="max-w-2xl space-y-6">
        <NotificationChannelsSection isLoading={isLoading} notifications={notifications} />
        <NotificationGroupsSection groups={groups} notifications={notifications} />
      </div>
    </div>
  )
}
