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
  },
  {
    value: "INACTIVE",
    label: "Inactive",
    icon: MonitorDot,
  },
  {
    value: "DISABLED",
    label: "Disabled",
    icon: MonitorXIcon,
  },
  {
    value: "UNKNOWN",
    label: "Unknown",
    icon: MonitorPauseIcon,
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
