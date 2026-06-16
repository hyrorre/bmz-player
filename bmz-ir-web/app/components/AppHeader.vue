<script setup lang="ts">
const { open } = useAppSidebar()

const headerUi = {
  root: 'h-auto',
  container: 'h-auto',
  header: 'h-auto',
  title: 'py-2',
}
</script>

<template>
  <UHeader title="BMZ IR" to="/" toggle-side="left" :toggle="true" :ui="headerUi">
    <template #toggle>
      <UButton
        icon="i-lucide-menu"
        color="neutral"
        variant="ghost"
        class="lg:hidden"
        aria-label="メニューを開く"
        @click="open = !open"
      />
    </template>
  </UHeader>
</template>
