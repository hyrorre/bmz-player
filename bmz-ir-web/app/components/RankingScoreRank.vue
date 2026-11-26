<script setup lang="ts">
const props = defineProps<{ score: number | null; max?: number | null }>()
const display = computed(() =>
  props.score === null ? null : rankingScoreDisplay(props.score, props.max),
)
</script>

<template>
  <div class="whitespace-nowrap tabular-nums">
    <IrRankBadge :label="display?.grade ?? '-'" size="sm" />
    <p v-if="display" class="mt-0.5 text-xs text-muted">({{ display.difference }})</p>
  </div>
</template>
