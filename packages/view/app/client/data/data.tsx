import {
  ArrowDownIcon,
  ArrowRightIcon,
  ArrowUpIcon,
} from "@radix-ui/react-icons"
import {
  MonitorCheck,
  MonitorDot,
  MonitorPauseIcon,
  MonitorXIcon
} from "lucide-react";

export const labels = [
  {
    value: "bug",
    label: "Bug",
  },
  {
    value: "feature",
    label: "Feature",
  },
  {
    value: "documentation",
    label: "Documentation",
  },
]

export const statuses = [
  {
    value: "ACTIVE",
    label: "Active",
    icon: MonitorCheck,
    color: "text-blue-500 dark:text-blue-300",
  },
  {
    value: "INACTIVE",
    label: "Inactive",
    icon: MonitorDot,
    color: "text-orange-400 dark:text-orange-200"
  },
  {
    value: "DISABLED",
    label: "Disabled",
    icon: MonitorXIcon,
    color: "text-rose-400 dark:text-rose-300",
  },
  {
    value: "UNKNOWN",
    label: "Unknown",
    icon: MonitorPauseIcon,
    color: "text-muted-foreground",
  },
]

export const priorities = [
  {
    label: "Low",
    value: "low",
    icon: ArrowDownIcon,
  },
  {
    label: "Medium",
    value: "medium",
    icon: ArrowRightIcon,
  },
  {
    label: "High",
    value: "high",
    icon: ArrowUpIcon,
  },
]
