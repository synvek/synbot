import {
  Cable,
  Clock,
  LayoutDashboard,
  MessageSquare,
  MessagesSquare,
  Puzzle,
  ScrollText,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Users,
  Workflow,
  type LucideIcon,
} from 'lucide-react'

export interface NavItem {
  path: string
  labelKey: string
  icon: LucideIcon
}

export interface NavGroup {
  id: string
  labelKey: string
  items: NavItem[]
}

/**
 * The rail's information architecture. Paths are unchanged from the first console — `/chat`,
 * `/security`, and the rest are bookmarked and linked in the docs, so only grouping is new.
 */
export const navGroups: NavGroup[] = [
  {
    id: 'workspace',
    labelKey: 'nav.workspace',
    items: [
      { path: '/', labelKey: 'sidebar.overview', icon: LayoutDashboard },
      { path: '/chat', labelKey: 'sidebar.chat', icon: MessageSquare },
      { path: '/sessions', labelKey: 'sidebar.sessions', icon: MessagesSquare },
    ],
  },
  {
    id: 'automation',
    labelKey: 'nav.automation',
    items: [
      { path: '/channels', labelKey: 'sidebar.channels', icon: Cable },
      { path: '/cron', labelKey: 'sidebar.cron', icon: Clock },
      { path: '/workflows', labelKey: 'sidebar.workflows', icon: Workflow },
    ],
  },
  {
    id: 'extensions',
    labelKey: 'nav.extensions',
    items: [
      { path: '/roles', labelKey: 'sidebar.roles', icon: Users },
      { path: '/skills', labelKey: 'sidebar.skills', icon: Sparkles },
      { path: '/plugins', labelKey: 'sidebar.plugins', icon: Puzzle },
    ],
  },
  {
    id: 'system',
    labelKey: 'nav.system',
    items: [
      { path: '/security', labelKey: 'sidebar.security', icon: ShieldCheck },
      { path: '/config', labelKey: 'sidebar.config', icon: SlidersHorizontal },
      { path: '/logs', labelKey: 'sidebar.logs', icon: ScrollText },
    ],
  },
]

export const navItems: NavItem[] = navGroups.flatMap((group) => group.items)
