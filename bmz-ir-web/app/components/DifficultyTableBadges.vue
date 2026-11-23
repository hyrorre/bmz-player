<script setup lang="ts">
import type { IrDifficultyLabel } from '../../shared/types/ir'
import { difficultyFilterForLabel } from '../../shared/utils/difficulty_filter'

defineProps<{ labels: IrDifficultyLabel[]; q?: string }>()
const { t } = useI18n()
const localePath = useLocalePath()
</script>

<template>
  <div v-if="labels.length" class="flex flex-wrap gap-1">
    <NuxtLink
      v-for="label in labels"
      :key="`${label.table_id}-${label.level}`"
      :to="localePath({ path: '/charts', query: { ...difficultyFilterForLabel(label), q } })"
      :prefetch="false"
      :title="label.table_name"
      :aria-label="
        t('charts.filterByDifficulty', {
          difficulty: `${label.table_name} ${label.symbol}${label.level}`,
        })
      "
      class="inline-flex rounded hover:underline focus-visible:outline-2 focus-visible:outline-primary"
    >
      <UBadge color="primary" size="sm" variant="subtle"
        >{{ label.symbol }}{{ label.level }}</UBadge
      >
    </NuxtLink>
  </div>
</template>
