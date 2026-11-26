<script setup lang="ts">
const props = defineProps<{ value: string | null }>()
const { formatDateAndTime } = useLocaleFormat()
const display = computed(() => (props.value ? formatDateAndTime(props.value) : null))
</script>

<template>
  <time v-if="display" :datetime="value ?? undefined" class="block whitespace-nowrap tabular-nums">
    <span class="block">{{ display.date }}</span>
    <span class="block text-xs">{{ display.time }}</span>
  </time>
  <span v-else>-</span>
</template>
