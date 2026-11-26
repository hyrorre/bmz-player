<script setup lang="ts">
const props = defineProps<{ score: number; max?: number | null; rateDigits?: number }>()
const display = computed(() => rankingScoreDisplay(props.score, props.max))
</script>

<template>
  <div class="whitespace-nowrap tabular-nums">
    <p class="font-semibold">
      {{ score }} <span v-if="display" class="font-normal text-muted">/ {{ display.max }}</span>
    </p>
    <p v-if="display" class="mt-0.5 text-xs text-muted">
      ({{ display.rate.toFixed(rateDigits ?? 1) }}%)
    </p>
  </div>
</template>
