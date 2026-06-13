<script setup lang="ts">
const { user } = useUserSession()

const navigationItems = [
  { label: '譜面', icon: 'i-lucide-list-music', to: '/charts' },
  { label: 'コース', icon: 'i-lucide-medal', to: '/courses' },
]

const accountItems = computed(() =>
  user.value
    ? [
        { label: 'プロフィール', icon: 'i-lucide-user-pen', to: '/profile' },
        { label: '設定', icon: 'i-lucide-settings', to: '/settings' },
      ]
    : [
        { label: 'ログイン', icon: 'i-lucide-log-in', to: '/login' },
        { label: '登録', icon: 'i-lucide-user-plus', to: '/register' },
      ],
)
</script>

<template>
  <UHeader
    title="BMZ IR"
    to="/"
    class="border-neutral-800 bg-neutral-950/90 text-neutral-50"
    :ui="{ title: 'text-neutral-50', center: 'flex-1 justify-start' }"
  >
    <UNavigationMenu :items="navigationItems" />

    <template #right>
      <UNavigationMenu :items="accountItems" class="hidden sm:flex" />
      <UButton v-if="user" color="neutral" icon="i-lucide-log-out" to="/logout" variant="ghost" />
    </template>

    <template #body>
      <UNavigationMenu
        :items="[navigationItems, accountItems]"
        class="-mx-2.5"
        orientation="vertical"
      />
      <UButton
        v-if="user"
        class="mt-4"
        color="neutral"
        icon="i-lucide-log-out"
        to="/logout"
        variant="subtle"
      >
        ログアウト
      </UButton>
    </template>
  </UHeader>
</template>
