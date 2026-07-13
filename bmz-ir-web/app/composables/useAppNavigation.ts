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
  const router = useRouter()

  const logout = async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
    router.push('/')
  }

  const navigationItems: NavigationMenuItem[] = [
    { label: '譜面', icon: 'i-lucide-list-music', to: '/charts' },
    { label: 'コース', icon: 'i-lucide-medal', to: '/courses' },
    { label: 'ユーザー', icon: 'i-lucide-users', to: '/players' },
  ]

  const accountItems = computed<NavigationMenuItem[]>(() =>
    user.value
      ? [
          {
            label: user.value.displayName ?? 'プロフィール',
            icon: 'i-lucide-user',
            defaultOpen: true,
            children: [
              { label: '本日の成果', to: '/daily', icon: 'i-lucide-calendar-days' },
              { label: 'プロフィール', to: '/profile', icon: 'i-lucide-user-pen' },
              { label: 'アカウント設定', to: '/settings', icon: 'i-lucide-settings' },
              {
                label: 'ログアウト',
                icon: 'i-lucide-log-out',
                onSelect: () => logout(),
              },
            ],
          },
        ]
      : [
          { label: 'ログイン', icon: 'i-lucide-log-in', to: '/login' },
          { label: '登録', icon: 'i-lucide-user-plus', to: '/register' },
        ],
  )

  const sidebarMenuItems = computed(() => [navigationItems, accountItems.value])

  return {
    navigationItems,
    accountItems,
    sidebarMenuItems,
    logout,
  }
}
