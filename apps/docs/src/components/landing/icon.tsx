import {
  Activity,
  Bell,
  ChartColumn,
  ChartNoAxesColumn,
  Check,
  ChevronDown,
  ChevronLeft,
  Copy,
  Cpu,
  Download,
  Ellipsis,
  Eye,
  Github,
  LayoutDashboard,
  Lock,
  LockOpen,
  type LucideIcon,
  MemoryStick,
  Moon,
  PanelLeft,
  Pause,
  Pencil,
  Play,
  Plus,
  Route,
  Search,
  Server,
  Settings,
  Shield,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
  Trash2,
  Wifi
} from 'lucide-react'

/** Every icon the landing uses, keyed by its lucide name (the design's `icon-<name>` class). */
const icons = {
  activity: Activity,
  bell: Bell,
  'chart-column': ChartColumn,
  'chart-no-axes-column': ChartNoAxesColumn,
  check: Check,
  'chevron-down': ChevronDown,
  'chevron-left': ChevronLeft,
  copy: Copy,
  cpu: Cpu,
  download: Download,
  ellipsis: Ellipsis,
  eye: Eye,
  github: Github,
  'layout-dashboard': LayoutDashboard,
  lock: Lock,
  'lock-open': LockOpen,
  'memory-stick': MemoryStick,
  moon: Moon,
  'panel-left': PanelLeft,
  pause: Pause,
  pencil: Pencil,
  play: Play,
  plus: Plus,
  route: Route,
  search: Search,
  server: Server,
  settings: Settings,
  shield: Shield,
  'shield-check': ShieldCheck,
  'sliders-horizontal': SlidersHorizontal,
  sun: Sun,
  'trash-2': Trash2,
  wifi: Wifi
} satisfies Record<string, LucideIcon>

export type IconName = keyof typeof icons

interface IconProps {
  /** Extra classes on the `<i>`, e.g. `del`, `np-eye`, `ios-wifi`, `th-sun`. */
  className?: string
  name: IconName
}

/**
 * A decorative icon: `<i class="ic …" aria-hidden="true"><svg class="lucide lucide-<name>" /></i>`.
 * The `<i>` keeps the design's `> i` selectors working; CSS sizes the SVG to 1em like the icon font.
 */
export function Icon({ name, className }: IconProps) {
  const Glyph = icons[name]
  return (
    <i aria-hidden="true" className={className ? `ic ${className}` : 'ic'}>
      <Glyph size="1em" />
    </i>
  )
}
