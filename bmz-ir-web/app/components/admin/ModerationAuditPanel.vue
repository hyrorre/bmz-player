<script setup lang="ts">
import type { AdminAuditLog, Pagination } from '../../types/moderation'

const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()

const targetType = ref<'all' | 'user' | 'chart'>('all')
const targetId = ref('')
const actorId = ref('')
const applied = ref({ targetId: '', actorId: '' })
const page = ref(1)
const pageSize = 50

const targetTypeItems = computed(() => [
  { label: t('admin.audit.allTargets'), value: 'all' as const },
  { label: t('admin.audit.targetUser'), value: 'user' as const },
  { label: t('admin.audit.targetChart'), value: 'chart' as const },
])

const { data, pending, error, refresh } = useFetch<{
  logs: AdminAuditLog[]
  pagination: Pagination
}>('/api/v1/admin/moderation/audit-logs', {
  query: computed(() => ({
    limit: pageSize,
    offset: (page.value - 1) * pageSize,
    ...(targetType.value === 'all' ? {} : { target_type: targetType.value }),
    ...(applied.value.targetId ? { target_id: applied.value.targetId } : {}),
    ...(applied.value.actorId ? { actor_id: applied.value.actorId } : {}),
  })),
  watch: [page, targetType],
})

watch(targetType, () => {
  page.value = 1
})

function applyFilter() {
  applied.value = { targetId: targetId.value.trim(), actorId: actorId.value.trim() }
  if (page.value === 1) refresh()
  else page.value = 1
}

function statusLabel(log: AdminAuditLog, status: string) {
  return log.target_type === 'user'
    ? t(`admin.userStatus.${status}`)
    : t(`admin.chartStatus.${status}`)
}

defineExpose({ refresh })
</script>

<template>
  <section>
    <div class="mb-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-[10rem_1fr_1fr_auto]">
      <USelect
        v-model="targetType"
        :items="targetTypeItems"
        :aria-label="t('admin.audit.targetType')"
      />
      <UInput
        v-model="targetId"
        :placeholder="t('admin.audit.targetId')"
        @keydown.enter="applyFilter"
      />
      <UInput
        v-model="actorId"
        :placeholder="t('admin.audit.actorId')"
        @keydown.enter="applyFilter"
      />
      <UButton color="primary" variant="subtle" icon="i-lucide-filter" @click="applyFilter">
        {{ t('admin.audit.filter') }}
      </UButton>
    </div>

    <UAlert
      v-if="error"
      color="error"
      :description="translateApiError(error, 'admin.loadFailed')"
    />
    <p v-else-if="pending && !data" class="text-sm text-muted">{{ t('common.loading') }}</p>
    <p v-else-if="!data?.logs.length" class="text-sm text-muted">{{ t('admin.audit.empty') }}</p>
    <div v-else class="overflow-x-auto rounded-lg border border-default">
      <table class="w-full min-w-[48rem] text-sm">
        <thead class="bg-elevated text-left text-toned">
          <tr>
            <th class="px-3 py-2">{{ t('admin.audit.date') }}</th>
            <th class="px-3 py-2">{{ t('admin.audit.actor') }}</th>
            <th class="px-3 py-2">{{ t('admin.audit.action') }}</th>
            <th class="px-3 py-2">{{ t('admin.audit.target') }}</th>
            <th class="px-3 py-2">{{ t('admin.audit.change') }}</th>
            <th class="px-3 py-2">{{ t('admin.reason') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="log in data.logs" :key="log.id" class="border-t border-default align-top">
            <td class="px-3 py-2 whitespace-nowrap text-muted">
              {{ formatDateTime(log.created_at) }}
            </td>
            <td class="px-3 py-2">
              <p>{{ log.actor.display_name || log.actor.id }}</p>
              <p class="font-mono text-xs text-dimmed">{{ log.actor.id }}</p>
            </td>
            <td class="px-3 py-2 whitespace-nowrap">{{ t(`admin.auditActions.${log.action}`) }}</td>
            <td class="max-w-56 px-3 py-2">
              <p class="truncate">
                {{
                  log.target_type === 'user'
                    ? t('admin.audit.targetUser')
                    : t('admin.audit.targetChart')
                }}
                · {{ log.target_label || '-' }}
              </p>
              <p class="truncate font-mono text-xs text-dimmed">{{ log.target_id }}</p>
            </td>
            <td class="px-3 py-2 whitespace-nowrap">
              {{ statusLabel(log, log.previous_status) }} → {{ statusLabel(log, log.new_status) }}
            </td>
            <td class="max-w-72 px-3 py-2 break-words">{{ log.reason }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="data && data.pagination.total > pageSize" class="mt-4 flex justify-end">
      <UPagination v-model:page="page" :items-per-page="pageSize" :total="data.pagination.total" />
    </div>
  </section>
</template>
