<script setup lang="ts">
const { sidebarMenuItems } = useAppNavigation()
const { open } = useAppSidebar()
</script>

<template>
  <USidebar
    v-model:open="open"
    collapsible="icon"
    :ui="{
      container: 'static h-full',
    }"
  >
    <template #default="{ state }">
      <UNavigationMenu
        :collapsed="state === 'collapsed'"
        :items="sidebarMenuItems"
        :popover="state === 'collapsed'"
        orientation="vertical"
        :ui="appNavigationMenuUi"
      />
    </template>
  </USidebar>
</template>
