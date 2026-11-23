<script setup lang="ts">
import type { RouteLocationRaw } from 'vue-router'

const props = defineProps<{
  page: number
  pageSize: number
  maxPage: number
  total: number
  totalLabel?: string
  pageLocation: (page: number) => RouteLocationRaw
}>()
const { t } = useI18n()
const { formatNumber } = useLocaleFormat()
const navigationTotal = computed(() => Math.min(props.total, props.maxPage * props.pageSize))
const lastPage = computed(() => Math.max(1, Math.ceil(navigationTotal.value / props.pageSize)))
</script>

<template>
  <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
    <p class="text-sm text-muted">
      {{ totalLabel ?? t('ranking.totalPlayers', { total: formatNumber(total) }) }}
    </p>
    <UPagination
      v-if="lastPage > 1 || page > 1"
      :page="Math.min(page, lastPage)"
      :to="pageLocation"
      :items-per-page="pageSize"
      :total="Math.max(1, navigationTotal)"
      :sibling-count="1"
      :show-controls="lastPage > 1"
      size="sm"
    />
  </div>
</template>
