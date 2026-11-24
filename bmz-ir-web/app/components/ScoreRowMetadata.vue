<script setup lang="ts">
defineProps<{ device: string | null; date: string | null }>()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
</script>

<template>
  <dl class="flex flex-wrap gap-x-6 gap-y-2 text-sm">
    <div>
      <dt class="text-xs text-muted">{{ t('table.input') }}</dt>
      <dd>{{ device ?? '-' }}</dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{{ t('table.date') }}</dt>
      <dd>{{ date ? formatDateTime(date) : '-' }}</dd>
    </div>
  </dl>
</template>
