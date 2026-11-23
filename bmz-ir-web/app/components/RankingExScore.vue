<script setup lang="ts">
const props = defineProps<{ score: number; max?: number | null }>()
const display = computed(() => rankingScoreDisplay(props.score, props.max))
</script>

<template>
  <div class="whitespace-nowrap tabular-nums">
    <p class="font-semibold">
      {{ score }} <span v-if="display" class="font-normal text-muted">/ {{ display.max }}</span>
    </p>
    <p v-if="display" class="mt-0.5 flex items-center justify-end gap-1.5 text-xs text-muted">
      <span>({{ display.rate.toFixed(1) }}%)</span>
      <UBadge color="warning" size="sm" variant="subtle">{{ display.grade }}</UBadge>
      <span>({{ display.difference }})</span>
    </p>
  </div>
</template>
