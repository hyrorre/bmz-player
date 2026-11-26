<script setup lang="ts">
defineProps<{ expanded: boolean; controls: string }>()
const emit = defineEmits<{ toggle: [] }>()
const { t } = useI18n()
</script>

<template>
  <UButton
    type="button"
    icon="i-lucide-ellipsis"
    color="neutral"
    :variant="expanded ? 'subtle' : 'ghost'"
    :aria-expanded="expanded"
    :aria-controls="expanded ? controls : undefined"
    :aria-label="t('score.title')"
    :title="t('score.title')"
    @click="emit('toggle')"
  />
</template>
