import type { NavigationMenuItem } from '@nuxt/ui'

/** サイドバーの開閉状態を管理するための、子項目を持つ項目の値。 */
export const APP_NAV_MANUAL_VALUE = 'manual'
export const APP_NAV_ACCOUNT_VALUE = 'account'

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
  const { pages: manualPageList, pagePath: manualPagePath } = useManual()
  const { isAdmin } = useIrAccount()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    await navigateTo(localePath('/'))
  }

  const navigationItems = computed<NavigationMenuItem[]>(() => [
    { label: t('nav.charts'), icon: 'i-lucide-list-music', to: localePath('/charts') },
    { label: t('nav.courses'), icon: 'i-lucide-medal', to: localePath('/courses') },
    { label: t('nav.players'), icon: 'i-lucide-users', to: localePath('/players') },
    {
      label: t('nav.manual'),
      icon: 'i-lucide-book-open',
      to: localePath('/manual'),
      value: APP_NAV_MANUAL_VALUE,
      children: manualPageList.value.map((page) => ({
        label: page.title,
        to: manualPagePath(page.slug),
      })),
    },
  ])

  const accountItems = computed<NavigationMenuItem[]>(() =>
    user.value
      ? [
          {
            label: user.value.displayName ?? t('nav.profile'),
            icon: 'i-lucide-user',
            value: APP_NAV_ACCOUNT_VALUE,
            children: [
              { label: t('nav.daily'), to: localePath('/daily'), icon: 'i-lucide-calendar-days' },
              { label: t('nav.rivals'), to: localePath('/rivals'), icon: 'i-lucide-users-round' },
              { label: t('nav.profile'), to: localePath('/profile'), icon: 'i-lucide-user-pen' },
              { label: t('nav.settings'), to: localePath('/settings'), icon: 'i-lucide-settings' },
              ...(isAdmin.value
                ? [
                    {
                      label: t('admin.nav'),
                      to: localePath('/admin/moderation'),
                      icon: 'i-lucide-shield',
                    },
                  ]
                : []),
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
