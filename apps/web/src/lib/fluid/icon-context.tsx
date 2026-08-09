'use client'

import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Bell,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock,
  Copy,
  CornerDownRight,
  Dot,
  Globe,
  Heart,
  Home,
  ImageIcon,
  Inbox,
  Lightbulb,
  Link,
  Loader,
  Lock,
  Mail,
  Menu,
  MessageCircle,
  Monitor,
  Moon,
  Paintbrush,
  Palette,
  Pause,
  Pencil,
  Pipette,
  Play,
  Plus,
  RectangleHorizontal,
  Rocket,
  RotateCcw,
  Search,
  Settings,
  Shield,
  SkipForward,
  SquareLibrary,
  Star,
  Sun,
  User,
  Users,
  X
} from 'lucide-react'
import { type ComponentType, createContext, type ReactNode, useContext, useMemo } from 'react'

export interface IconComponentProps {
  className?: string
  size?: number
  strokeWidth?: number
}

export type IconComponent = ComponentType<IconComponentProps>

export type IconName =
  | 'chevron-right'
  | 'chevron-down'
  | 'x'
  | 'copy'
  | 'menu'
  | 'dot'
  | 'monitor'
  | 'sun'
  | 'moon'
  | 'rectangle-horizontal'
  | 'circle'
  | 'square-library'
  | 'clock'
  | 'star'
  | 'settings'
  | 'plus'
  | 'arrow-left'
  | 'arrow-right'
  | 'arrow-up'
  | 'search'
  | 'loader'
  | 'users'
  | 'lock'
  | 'mail'
  | 'bell'
  | 'shield'
  | 'palette'
  | 'lightbulb'
  | 'rocket'
  | 'heart'
  | 'paintbrush'
  | 'brain'
  | 'globe'
  | 'user'
  | 'image'
  | 'link'
  | 'check'
  | 'rotate-ccw'
  | 'play'
  | 'pause'
  | 'pipette'
  | 'home'
  | 'message-circle'
  | 'inbox'
  | 'pencil'
  | 'skip-forward'
  | 'corner-down-right'

export const defaultIcons: Record<IconName, IconComponent> = {
  'chevron-right': ChevronRight,
  'chevron-down': ChevronDown,
  pipette: Pipette,
  x: X,
  copy: Copy,
  menu: Menu,
  dot: Dot,
  monitor: Monitor,
  sun: Sun,
  moon: Moon,
  'rectangle-horizontal': RectangleHorizontal,
  circle: Circle,
  'square-library': SquareLibrary,
  clock: Clock,
  star: Star,
  settings: Settings,
  plus: Plus,
  'arrow-left': ArrowLeft,
  'arrow-right': ArrowRight,
  'arrow-up': ArrowUp,
  search: Search,
  loader: Loader,
  users: Users,
  lock: Lock,
  mail: Mail,
  bell: Bell,
  shield: Shield,
  palette: Palette,
  lightbulb: Lightbulb,
  rocket: Rocket,
  heart: Heart,
  paintbrush: Paintbrush,
  brain: Brain,
  globe: Globe,
  user: User,
  image: ImageIcon,
  link: Link,
  check: Check,
  'rotate-ccw': RotateCcw,
  play: Play,
  pause: Pause,
  home: Home,
  'message-circle': MessageCircle,
  inbox: Inbox,
  pencil: Pencil,
  'skip-forward': SkipForward,
  'corner-down-right': CornerDownRight
}

const IconContext = createContext<Record<IconName, IconComponent> | null>(null)

/**
 * Returns a single icon component for the given name.
 * Falls back to the default (Lucide) set if no provider is present.
 */
function useIcon(name: IconName): IconComponent {
  const icons = useContext(IconContext)
  return (icons ?? defaultIcons)[name]
}

/**
 * Returns the full icon map.
 * Falls back to the default (Lucide) set if no provider is present.
 */
function useIcons(): Record<IconName, IconComponent> {
  const icons = useContext(IconContext)
  return icons ?? defaultIcons
}

/**
 * Swap some or all icons for components from another library.
 * Names left out of `icons` keep their default (Lucide) component.
 */
function IconProvider({ children, icons }: { children: ReactNode; icons?: Partial<Record<IconName, IconComponent>> }) {
  const value = useMemo(() => ({ ...defaultIcons, ...icons }), [icons])
  return <IconContext.Provider value={value}>{children}</IconContext.Provider>
}

export { IconProvider, useIcon, useIcons }
