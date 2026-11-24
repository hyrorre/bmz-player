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
    variant="subtle"
    class="whitespace-nowrap dark:[--ir-rank-badge-accent:80%]"
  >
    <slot>{{ label }}</slot>
  </UBadge>
</template>

<style scoped>
[data-rank] {
  background-color: color-mix(in srgb, var(--ir-rank-fill) 10%, transparent);
  color: color-mix(
    in oklab,
    var(--ir-rank-fill) var(--ir-rank-badge-accent, 45%),
    var(--ui-text-highlighted)
  );
  --tw-ring-color: color-mix(in srgb, var(--ir-rank-fill) 25%, transparent);
}
</style>
