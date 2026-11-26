<script setup lang="ts">
import type { NavigationMenuItem } from '@nuxt/ui'

const localePath = useLocalePath()
const { t } = useI18n()
const { navigationItems, accountMenu, accountItems } = useAppNavigation()
const open = ref(false)
const wideHeader = useViewportMatch('(min-width: 64rem)')

const headerUi = {
  root: 'h-auto',
  container: 'h-auto',
  header: 'h-auto',
  title: 'py-2',
}

const desktopMenuUi = {
  item: 'py-0',
  link: 'whitespace-nowrap',
  childLinkLabel: 'wrap-anywhere',
  viewportWrapper: 'mt-2',
  viewport: 'shrink-0',
}

watch(wideHeader, (wide) => {
  if (wide) open.value = false
})

function closeMenuOnSelect(item: NavigationMenuItem): NavigationMenuItem {
  if (item.children?.length) {
    return { ...item, children: item.children.map(closeMenuOnSelect) }
  }
  return {
    ...item,
    onSelect(event) {
      open.value = false
      item.onSelect?.(event)
    },
  }
}

const mobileItems = computed<NavigationMenuItem[][]>(() => [
  navigationItems.value.map(closeMenuOnSelect),
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
    <UNavigationMenu
      :items="navigationItems"
      content-orientation="vertical"
      :viewport="{ align: 'start' }"
      disable-hover-trigger
      disable-pointer-leave-close
      :ui="{ ...desktopMenuUi, linkLeadingIcon: 'hidden xl:block' }"
    />

    <template #right>
      <UNavigationMenu
        :items="accountMenu"
        :aria-label="t('nav.account')"
        class="hidden lg:flex"
        content-orientation="vertical"
        :viewport="{ align: 'end' }"
        disable-hover-trigger
        disable-pointer-leave-close
        :ui="{ ...desktopMenuUi, linkLabel: 'sr-only xl:not-sr-only xl:max-w-32 xl:truncate' }"
      />
      <LocaleSelect />
      <UColorModeButton />
    </template>

    <template #body>
      <UNavigationMenu :items="mobileItems" orientation="vertical" class="w-full" />
    </template>
  </UHeader>
</template>
