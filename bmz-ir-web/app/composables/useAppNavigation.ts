import type { NavigationMenuItem } from '@nuxt/ui'

type AppMenuItem = Pick<NavigationMenuItem, 'label' | 'icon' | 'to' | 'onSelect'>

export function useAppNavigation() {
  const { user, clear } = useUserSession()
  const localePath = useLocalePath()
  const { t } = useI18n()
  const { isAdmin } = useIrAccount()
  const { pages, pagePath } = useManual()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    await navigateTo(localePath('/'))
  }

  const navigationItems = computed<AppMenuItem[]>(() => [
    { label: t('nav.charts'), icon: 'i-lucide-list-music', to: localePath('/charts') },
    { label: t('lamps.nav'), icon: 'i-lucide-chart-bar-stacked', to: localePath('/lamps') },
    { label: t('nav.courses'), icon: 'i-lucide-medal', to: localePath('/courses') },
    { label: t('nav.players'), icon: 'i-lucide-users', to: localePath('/players') },
  ])

  const manualItems = computed<AppMenuItem[]>(() => [
    { label: t('manual.contents'), icon: 'i-lucide-list', to: localePath('/manual') },
    ...pages.value.map((page) => ({ label: page.title, to: pagePath(page.slug) })),
  ])

  const accountItems = computed<AppMenuItem[]>(() =>
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

  return { navigationItems, manualItems, accountItems }
}
