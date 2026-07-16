<script setup lang="ts">
const { open } = useAppSidebar()
const localePath = useLocalePath()
const { t } = useI18n()

const headerUi = {
  root: 'h-auto',
  container: 'h-auto',
  header: 'h-auto',
  title: 'py-2',
}
</script>

<template>
  <UHeader title="BMZ IR" :to="localePath('/')" toggle-side="left" :toggle="true" :ui="headerUi">
    <template #toggle>
      <UButton
        icon="i-lucide-menu"
        color="neutral"
        variant="ghost"
        class="lg:hidden"
        :aria-label="t('common.openMenu')"
        @click="open = !open"
      />
    </template>
    <template #right>
      <LocaleSelect />
      <UColorModeButton />
    </template>
  </UHeader>
</template>
