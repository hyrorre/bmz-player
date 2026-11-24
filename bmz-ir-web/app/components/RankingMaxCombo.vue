<script setup lang="ts">
defineProps<{ combo: number; totalNotes?: number | null }>()
</script>

<template>
  <span class="font-semibold whitespace-nowrap tabular-nums">
    {{ combo }}
    <span v-if="totalNotes && totalNotes >= combo" class="font-normal text-muted">
      / {{ totalNotes }}
    </span>
  </span>
</template>
