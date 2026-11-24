<script setup lang="ts">
import { CLEAR_RANK, IR_CLEAR_LABELS } from '../../shared/constants/ir'

const props = defineProps<{ clear?: string; rank?: number; compact?: boolean }>()
const label = computed(() => {
  if (!props.clear) return '-'
  return IR_CLEAR_LABELS[CLEAR_RANK[props.clear] ?? -1] ?? props.clear.toUpperCase()
})
const abbreviateFullCombo = computed(() => props.compact && label.value === 'FULL COMBO')
</script>

<template>
  <IrRankBadge :label="label" kind="clear" :rank="clear ? rank : undefined">
    <template v-if="abbreviateFullCombo" #default>
      <abbr :title="label" class="no-underline">FC</abbr>
    </template>
  </IrRankBadge>
</template>
