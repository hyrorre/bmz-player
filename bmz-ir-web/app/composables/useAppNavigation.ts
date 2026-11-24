import type { DropdownMenuItem, NavigationMenuItem } from '@nuxt/ui'

export function useAppNavigation() {
  const { user, clear } = useUserSession()
  const localePath = useLocalePath()
  const { t } = useI18n()
  const { isAdmin } = useIrAccount()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    await navigateTo(localePath('/'))
  }

  const navigationItems = computed<NavigationMenuItem[]>(() => [
    { label: t('nav.charts'), icon: 'i-lucide-list-music', to: localePath('/charts') },
    { label: t('lamps.nav'), icon: 'i-lucide-chart-bar-stacked', to: localePath('/lamps') },
    { label: t('nav.courses'), icon: 'i-lucide-medal', to: localePath('/courses') },
    { label: t('nav.players'), icon: 'i-lucide-users', to: localePath('/players') },
    { label: t('nav.manual'), icon: 'i-lucide-book-open', to: localePath('/manual') },
  ])

  const accountItems = computed<DropdownMenuItem[]>(() =>
    user.value
      ? [
          {
            label: t('nav.myPage'),
            to: localePath(`/players/${user.value.id}`),
            icon: 'i-lucide-circle-user-round',
          },
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
        ]
      : [
          { label: t('nav.login'), icon: 'i-lucide-log-in', to: localePath('/login') },
          { label: t('nav.register'), icon: 'i-lucide-user-plus', to: localePath('/register') },
        ],
  )

  return { navigationItems, accountItems }
}
