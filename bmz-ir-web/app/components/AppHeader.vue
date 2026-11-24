<script setup lang="ts">
const localePath = useLocalePath()
const { t } = useI18n()
const { user } = useUserSession()
const { navigationItems, accountItems } = useAppNavigation()
const open = ref(false)
const wideHeader = useViewportMatch('(min-width: 64rem)')

watch(wideHeader, (wide) => {
  if (wide) open.value = false
})

const mobileItems = computed(() =>
  [navigationItems.value, accountItems.value].map((group) =>
    group.map(({ onSelect, ...item }) => ({
      ...item,
      onSelect(event: Event) {
        open.value = false
        onSelect?.(event)
      },
    })),
  ),
)
</script>

<template>
  <UHeader
    v-model:open="open"
    title="BMZ IR"
    :to="localePath('/')"
    toggle-side="left"
    mode="slideover"
    :menu="{ side: 'left' }"
  >
    <UNavigationMenu
      :items="navigationItems"
      :ui="{ link: 'whitespace-nowrap', linkLeadingIcon: 'hidden xl:block' }"
    />

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
