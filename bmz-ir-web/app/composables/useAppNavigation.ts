import type { NavigationMenuItem } from '@nuxt/ui'

export const appNavigationMenuUi = {
  link: 'p-1.5 overflow-hidden',
}

export function useAppSidebar() {
  const open = useState('app-sidebar-open', () => true)

  return { open }
}

export function useAppNavigation() {
  const { user, clear } = useUserSession()
  const localePath = useLocalePath()
  const { t } = useI18n()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    await navigateTo(localePath('/'))
  }

  const navigationItems = computed<NavigationMenuItem[]>(() => [
    { label: t('nav.charts'), icon: 'i-lucide-list-music', to: localePath('/charts') },
    { label: t('nav.courses'), icon: 'i-lucide-medal', to: localePath('/courses') },
    { label: t('nav.players'), icon: 'i-lucide-users', to: localePath('/players') },
  ])

  const accountItems = computed<NavigationMenuItem[]>(() =>
    user.value
      ? [
          {
            label: user.value.displayName ?? t('nav.profile'),
            icon: 'i-lucide-user',
            defaultOpen: true,
            children: [
              { label: t('nav.daily'), to: localePath('/daily'), icon: 'i-lucide-calendar-days' },
              { label: t('nav.profile'), to: localePath('/profile'), icon: 'i-lucide-user-pen' },
              { label: t('nav.settings'), to: localePath('/settings'), icon: 'i-lucide-settings' },
              {
                label: t('nav.logout'),
                icon: 'i-lucide-log-out',
                onSelect: () => logout(),
              },
            ],
          },
        ]
      : [
          { label: t('nav.login'), icon: 'i-lucide-log-in', to: localePath('/login') },
          { label: t('nav.register'), icon: 'i-lucide-user-plus', to: localePath('/register') },
        ],
  )

  const sidebarMenuItems = computed(() => [navigationItems.value, accountItems.value])

  return {
    navigationItems,
    accountItems,
    sidebarMenuItems,
    logout,
  }
}
