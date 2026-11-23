<script setup lang="ts">
import { CLEAR_RANK, IR_CLEAR_LABELS } from '../../shared/constants/ir'

const props = defineProps<{ clear?: string; rank?: number }>()
const label = computed(() => {
  if (!props.clear) return '-'
  return IR_CLEAR_LABELS[CLEAR_RANK[props.clear] ?? -1] ?? props.clear.toUpperCase()
})
const color = computed(() => {
  const rank = props.rank ?? CLEAR_RANK[props.clear ?? ''] ?? 0
  if (rank >= 7) return 'warning'
  if (rank >= 5) return 'error'
  if (rank >= 3) return 'success'
  return 'neutral'
})
</script>

<template>
  <UBadge :color="color" variant="subtle" class="whitespace-nowrap">{{ label }}</UBadge>
</template>
