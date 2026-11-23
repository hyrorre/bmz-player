<script setup lang="ts">
import type { IrDifficultyLabel } from '../../shared/types/ir'

defineProps<{ labels: IrDifficultyLabel[] }>()
</script>

<template>
  <div v-if="labels.length" class="flex flex-wrap gap-1">
    <UBadge
      v-for="label in labels"
      :key="`${label.table_id}-${label.level}`"
      color="primary"
      size="sm"
      variant="subtle"
      :title="label.table_name"
    >
      {{ label.symbol }}{{ label.level }}
    </UBadge>
  </div>
</template>
