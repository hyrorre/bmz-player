import type { NavigationMenuChildItem, NavigationMenuItem } from '@nuxt/ui'

export function useAppNavigation() {
  const { user, clear } = useUserSession()
  const route = useRoute()
  const localePath = useLocalePath()
  const { t } = useI18n()
  const { isAdmin } = useIrAccount()
  const { pages, pagePath } = useManual()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    await navigateTo(localePath('/'))
  }

  const navigationItems = computed<NavigationMenuItem[]>(() => {
    const manualPath = localePath('/manual')
    return [
      { label: t('nav.charts'), icon: 'i-lucide-list-music', to: localePath('/charts') },
      { label: t('nav.courses'), icon: 'i-lucide-medal', to: localePath('/courses') },
      { label: t('nav.players'), icon: 'i-lucide-users', to: localePath('/players') },
      {
        value: 'manual',
        label: t('nav.manual'),
        icon: 'i-lucide-book-open',
        active: route.path === manualPath || route.path.startsWith(`${manualPath}/`),
        children: [
          { label: t('manual.contents'), icon: 'i-lucide-list', to: manualPath },
          ...pages.value.map((page) => ({ label: page.title, to: pagePath(page.slug) })),
        ],
      },
    ]
  })

  const accountItems = computed<NavigationMenuChildItem[]>(() =>
    user.value
      ? [
          {
            label: t('nav.myPage'),
            to: localePath(`/players/${user.value.id}`),
            icon: 'i-lucide-circle-user-round',
          },
          { label: t('nav.daily'), to: localePath('/daily'), icon: 'i-lucide-calendar-days' },
          {
            label: t('analysis.nav'),
            to: localePath('/analysis'),
            icon: 'i-lucide-chart-bar-stacked',
          },
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
        ]
      : [
          { label: t('nav.login'), icon: 'i-lucide-log-in', to: localePath('/login') },
          { label: t('nav.register'), icon: 'i-lucide-user-plus', to: localePath('/register') },
        ],
  )

  const accountMenu = computed<NavigationMenuItem[]>(() => [
    {
      value: 'account',
      label: user.value?.displayName || t('nav.account'),
      icon: 'i-lucide-user',
      children: accountItems.value,
    },
  ])

  return { navigationItems, accountMenu, accountItems }
}
