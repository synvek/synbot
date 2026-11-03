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
 * The one destination the shell is built around. It is not part of the admin menu: the app opens
 * here, the sidebar's primary action belongs to it, and everything else is reachable from 「更多」.
 */
export const primaryNav: NavItem = {
  path: '/chat',
  labelKey: 'sidebar.chat',
  icon: MessageSquare,
}

/**
 * Everything that administers the runtime. Grouped here and rendered by the sidebar's 「更多」
 * menu and the ⌘K palette; the sidebar itself only lists conversations, so a new admin page has to
 * be added in exactly one place.
 */
export const adminNavGroups: NavGroup[] = [
  {
    id: 'workspace',
    labelKey: 'nav.workspace',
    items: [
      { path: '/overview', labelKey: 'sidebar.overview', icon: LayoutDashboard },
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

/** Every destination, in menu order: the palette and the sidebar walk the same list. */
export const navItems: NavItem[] = [primaryNav, ...adminNavGroups.flatMap((group) => group.items)]
