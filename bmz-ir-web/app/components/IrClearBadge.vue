<script setup lang="ts">
import { CLEAR_RANK, IR_CLEAR_LABELS } from '../../shared/constants/ir'

const props = defineProps<{ clear?: string; rank?: number }>()
const label = computed(() => {
  if (!props.clear) return '-'
  return IR_CLEAR_LABELS[CLEAR_RANK[props.clear] ?? -1] ?? props.clear.toUpperCase()
})
</script>

<template>
  <IrRankBadge :label="label" kind="clear" :rank="clear ? rank : undefined" />
</template>
