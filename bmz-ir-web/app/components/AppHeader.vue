<script setup lang="ts">
import type { NavigationMenuItem } from '@nuxt/ui'

const route = useRoute()
const localePath = useLocalePath()
const { t } = useI18n()
const { user } = useUserSession()
const { navigationItems, manualItems, accountItems } = useAppNavigation()
const open = ref(false)
const wideHeader = useViewportMatch('(min-width: 64rem)')
const manualActive = computed(() => {
  const path = localePath('/manual')
  return route.path === path || route.path.startsWith(`${path}/`)
})

const headerUi = {
  root: 'h-auto',
  container: 'h-auto',
  header: 'h-auto',
  title: 'py-2',
}

watch(wideHeader, (wide) => {
  if (wide) open.value = false
})

function closeMenuOnSelect({ onSelect, ...item }: NavigationMenuItem): NavigationMenuItem {
  return {
    ...item,
    onSelect(event) {
      open.value = false
      onSelect?.(event)
    },
  }
}

const mobileItems = computed<NavigationMenuItem[][]>(() => [
  [
    ...navigationItems.value.map(closeMenuOnSelect),
    {
      label: t('nav.manual'),
      icon: 'i-lucide-book-open',
      active: manualActive.value,
      children: manualItems.value.map(closeMenuOnSelect),
    },
  ],
  accountItems.value.map(closeMenuOnSelect),
])
</script>

<template>
  <UHeader
    v-model:open="open"
    title="BMZ IR"
    :to="localePath('/')"
    toggle-side="left"
    mode="slideover"
    :menu="{ side: 'left' }"
    :ui="headerUi"
  >
    <div class="flex items-center">
      <UNavigationMenu
        :items="navigationItems"
        :ui="{ item: 'py-0', link: 'whitespace-nowrap', linkLeadingIcon: 'hidden xl:block' }"
      />
      <UDropdownMenu
        :items="manualItems"
        :content="{ align: 'start' }"
        :ui="{ content: 'min-w-64 max-w-80', itemLabel: 'whitespace-normal' }"
      >
        <UButton
          color="neutral"
          :variant="manualActive ? 'subtle' : 'ghost'"
          icon="i-lucide-book-open"
          trailing-icon="i-lucide-chevron-down"
          :ui="{ leadingIcon: 'hidden xl:block' }"
          class="whitespace-nowrap"
        >
          {{ t('nav.manual') }}
        </UButton>
      </UDropdownMenu>
    </div>

    <template #right>
      <UDropdownMenu :items="accountItems" :content="{ align: 'end' }">
        <UButton
          color="neutral"
          variant="ghost"
          icon="i-lucide-user"
          trailing-icon="i-lucide-chevron-down"
          :aria-label="t('nav.account')"
          class="hidden lg:inline-flex"
        >
          <span class="hidden max-w-32 truncate xl:inline">
            {{ user?.displayName ?? t('nav.account') }}
          </span>
        </UButton>
      </UDropdownMenu>
      <LocaleSelect />
      <UColorModeButton />
    </template>

    <template #body>
      <UNavigationMenu :items="mobileItems" orientation="vertical" class="w-full" />
    </template>
  </UHeader>
</template>
