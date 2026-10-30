<script setup lang="ts">
import {
  MAX_MODERATION_NOTE_LENGTH,
  MAX_MODERATION_REASON_LENGTH,
} from '#shared/constants/moderation'

const props = defineProps<{
  statusLabel: string
  targets: string[]
  pending: boolean
  withNote?: boolean
  initialNote?: string
}>()
const emit = defineEmits<{ confirm: [payload: { reason: string; note?: string }] }>()
const open = defineModel<boolean>('open', { required: true })
const { t } = useI18n()
const reason = ref('')
const note = ref('')

watch(open, (value) => {
  if (value) {
    reason.value = ''
    note.value = props.initialNote ?? ''
  }
})

const canSubmit = computed(() => reason.value.trim().length > 0 && !props.pending)

function submit() {
  if (!canSubmit.value) return
  emit('confirm', {
    reason: reason.value.trim(),
    ...(props.withNote ? { note: note.value.trim() } : {}),
  })
}
</script>

<template>
  <UModal
    v-model:open="open"
    :title="t('admin.confirm.title')"
    :description="t('admin.confirm.description', { count: targets.length, status: statusLabel })"
    :dismissible="!pending"
  >
    <template #body>
      <p class="mb-2 text-sm font-medium">{{ t('admin.confirm.targets') }}</p>
      <ul
        class="mb-4 max-h-40 overflow-y-auto rounded-md border border-default px-3 py-2 text-sm text-muted"
      >
        <li v-for="target in targets" :key="target" class="truncate">{{ target }}</li>
      </ul>
      <UFormField :label="t('admin.confirm.reason')" required class="mb-4">
        <UTextarea
          v-model="reason"
          class="w-full"
          :rows="3"
          :maxlength="MAX_MODERATION_REASON_LENGTH"
          :placeholder="t('admin.confirm.reasonPlaceholder')"
        />
      </UFormField>
      <UFormField v-if="withNote" :label="t('admin.charts.note')">
        <UTextarea
          v-model="note"
          class="w-full"
          :rows="2"
          :maxlength="MAX_MODERATION_NOTE_LENGTH"
          :placeholder="t('admin.charts.notePlaceholder')"
        />
      </UFormField>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" :disabled="pending" @click="open = false">
          {{ t('admin.confirm.cancel') }}
        </UButton>
        <UButton color="error" :loading="pending" :disabled="!canSubmit" @click="submit">
          {{ t('admin.confirm.submit') }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>
