import type { Href } from 'expo-router'
import type { ImageSourcePropType } from 'react-native'

import exploreIcon from '@/assets/images/tabIcons/explore.png'
import homeIcon from '@/assets/images/tabIcons/home.png'

export type TabDef = {
  /** Route name within the `(tabs)` group. */
  name: string
  /** Web href for the `expo-router/ui` tab trigger. */
  href: Href
  label: string
  icon: ImageSourcePropType
}

/**
 * Single source of truth for the bottom tabs, consumed by both the native
 * (`app-tabs.tsx`) and web (`app-tabs.web.tsx`) renderers so the tab list
 * can't drift between platforms.
 */
export const TABS: readonly TabDef[] = [
  { name: '(home)', href: '/', label: 'Instances', icon: homeIcon },
  { name: 'settings', href: '/settings', label: 'Settings', icon: exploreIcon },
]
