<script setup lang="ts">
import { IR_CLEAR_LABELS, IR_SCORE_RANK_LABELS } from '../../shared/constants/ir'

const props = defineProps<{ label: string; kind?: 'clear' | 'score'; rank?: number }>()
const paletteIndex = computed(() => {
  const labels = props.kind === 'clear' ? IR_CLEAR_LABELS : IR_SCORE_RANK_LABELS
  const index = props.rank ?? labels.findIndex((label) => label === props.label)
  return labels[index] === undefined ? undefined : index
})
</script>

<template>
  <UBadge
    :data-rank="paletteIndex"
    color="neutral"
    :variant="paletteIndex === undefined ? 'subtle' : 'solid'"
    class="whitespace-nowrap"
  >
    {{ label }}
  </UBadge>
</template>

<style scoped>
[data-rank] {
  background-color: var(--ir-rank-fill);
  color: var(--ir-rank-text);
}
</style>
