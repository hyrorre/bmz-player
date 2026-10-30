<script setup lang="ts">
const props = defineProps<{ kind: 'user' | 'chart'; status: string }>()
const { t } = useI18n()

const color = computed(() => {
  switch (props.status) {
    case 'banned':
    case 'blocked':
      return 'error' as const
    case 'submit_suspended':
    case 'hidden':
      return 'warning' as const
    case 'under_review':
      return 'info' as const
    default:
      return 'neutral' as const
  }
})
const label = computed(() =>
  props.kind === 'user'
    ? t(`admin.userStatus.${props.status}`)
    : t(`admin.chartStatus.${props.status}`),
)
</script>

<template>
  <UBadge :color="color" size="sm" variant="subtle">{{ label }}</UBadge>
</template>
