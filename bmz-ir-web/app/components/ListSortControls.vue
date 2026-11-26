<script setup lang="ts">
import type { ListSortOrder } from '../../shared/utils/list_sort'

defineProps<{ items: { value: string; label: string }[] }>()
const sort = defineModel<string>('sort', { required: true })
const order = defineModel<ListSortOrder>('order', { required: true })
const { t } = useI18n()
</script>

<template>
  <div class="flex items-end gap-2">
    <UFormField :label="t('listSort.label')" class="min-w-0 flex-1 sm:max-w-60">
      <USelect v-model="sort" :items="items" class="w-full" />
    </UFormField>
    <UButton
      type="button"
      color="neutral"
      variant="subtle"
      :icon="order === 'asc' ? 'i-lucide-arrow-up' : 'i-lucide-arrow-down'"
      :label="order === 'asc' ? t('listSort.asc') : t('listSort.desc')"
      :title="t('listSort.toggleDirection')"
      @click="order = order === 'asc' ? 'desc' : 'asc'"
    />
  </div>
</template>
