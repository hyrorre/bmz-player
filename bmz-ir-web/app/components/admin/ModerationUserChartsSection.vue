<script setup lang="ts">
import {
  CHART_MODERATION_STATUSES,
  MAX_CHART_MODERATION_BATCH_SIZE,
  type ChartModerationStatus,
} from '#shared/constants/moderation'
import type { AdminUserChartItem, ModerationChangeResult, Pagination } from '../../types/moderation'

/**
 * ユーザー詳細から、そのユーザーがスコアを送った譜面をまとめて制限する。1人のユーザーが
 * 大量に登録した違法譜面を、譜面タブで1件ずつ探さずに処理するためのもの。選択・確認・競合の
 * 扱いは譜面タブの一括操作と同じで、選択した時点の revision で送信する。
 */
const props = defineProps<{ userId: string }>()
const emit = defineEmits<{ changed: [] }>()
const { t } = useI18n()
const localePath = useLocalePath()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { notifyResults, notifyError } = useModerationResultToast()

const pageSize = MAX_CHART_MODERATION_BATCH_SIZE
const page = ref(1)
// 操作後に親がユーザー詳細を読み直すとこの部品は作り直されるため、絞り込みは親に保持する。
const soleOnly = defineModel<boolean>('soleOnly', { default: false })
const bulk = useBulkChartSelection()
const { selected, conflicts } = bulk

const {
  data: list,
  pending,
  error,
  refresh,
} = await useFetch<{ charts: AdminUserChartItem[]; pagination: Pagination }>(
  () => `/api/v1/admin/moderation/users/${encodeURIComponent(props.userId)}/charts`,
  {
    query: computed(() => ({
      limit: pageSize,
      offset: (page.value - 1) * pageSize,
      ...(soleOnly.value ? { sole: '1' } : {}),
    })),
    watch: [page, soleOnly],
  },
)

// 対象のユーザーが変わったら、別のユーザーの譜面を選んだまま操作しないよう選択を捨てる。
watch(
  () => props.userId,
  () => {
    page.value = 1
    bulk.clear()
    bulk.dismissConflicts()
  },
)
watch(soleOnly, () => {
  page.value = 1
})

function chartLabel(chart: { title: string; sha256: string }) {
  return `${chart.title || t('admin.charts.unregistered')} (${chart.sha256.slice(0, 12)}…)`
}

function toggle(chart: AdminUserChartItem, value: boolean | 'indeterminate') {
  if (value === true) bulk.select(chart, chartLabel(chart))
  else bulk.deselect(chart.sha256)
}

const pageAllSelected = computed(
  () => Boolean(list.value?.charts.length) && list.value!.charts.every((c) => bulk.has(c.sha256)),
)

function togglePage(value: boolean | 'indeterminate') {
  for (const chart of list.value?.charts ?? []) toggle(chart, value)
}

const overLimit = computed(() => selected.value.size > MAX_CHART_MODERATION_BATCH_SIZE)

interface PendingChartAction {
  readonly status: ChartModerationStatus
  readonly targets: ReadonlyArray<Readonly<{ sha256: string; label: string; revision: number }>>
}

const confirmOpen = ref(false)
const pendingAction = shallowRef<PendingChartAction | null>(null)
const submitting = ref(false)

function openConfirm(status: ChartModerationStatus) {
  if (submitting.value || selected.value.size === 0 || overLimit.value) return
  pendingAction.value = Object.freeze({
    status,
    targets: Object.freeze(bulk.snapshot().map((target) => Object.freeze(target))),
  })
  confirmOpen.value = true
}

watch(confirmOpen, (open) => {
  if (!open && !submitting.value) pendingAction.value = null
})

async function applyStatus({ reason, note }: { reason: string; note?: string }) {
  const action = pendingAction.value
  if (!action || submitting.value) return
  submitting.value = true
  try {
    const response = await $fetch<{
      results: Array<ModerationChangeResult<ChartModerationStatus> & { sha256: string }>
    }>('/api/v1/admin/moderation/charts/status', {
      method: 'POST',
      body: {
        items: action.targets.map((target) => ({
          sha256: target.sha256,
          expected_revision: target.revision,
        })),
        status: action.status,
        reason,
        note,
      },
    })
    notifyResults(response.results)
    bulk.applyResults(
      response.results,
      new Map(action.targets.map((target) => [target.sha256, target.label])),
    )
  } catch (error) {
    notifyError(error)
  } finally {
    submitting.value = false
    confirmOpen.value = false
    pendingAction.value = null
    await refresh()
    emit('changed')
  }
}

function actionColor(status: ChartModerationStatus) {
  switch (status) {
    case 'blocked':
      return 'error' as const
    case 'hidden':
      return 'warning' as const
    case 'under_review':
      return 'info' as const
    default:
      return 'primary' as const
  }
}
</script>

<template>
  <div>
    <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
      <h3 class="font-semibold">{{ t('admin.users.playedCharts') }}</h3>
      <USwitch v-model="soleOnly" size="sm" :label="t('admin.users.soleOnly')" />
    </div>
    <p class="mb-3 text-xs text-muted">{{ t('admin.users.playedChartsDescription') }}</p>

    <div
      class="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-default bg-default/90 px-3 py-2"
    >
      <UCheckbox
        :model-value="pageAllSelected"
        :label="t('admin.charts.selectPage')"
        :disabled="!list?.charts.length"
        @update:model-value="togglePage"
      />
      <span class="text-sm text-muted">
        {{ t('admin.charts.selected', { count: selected.size }) }}
      </span>
      <UButton v-if="selected.size" size="xs" color="neutral" variant="ghost" @click="bulk.clear()">
        {{ t('admin.charts.clearSelection') }}
      </UButton>
      <div class="ml-auto flex flex-wrap gap-1">
        <UButton
          v-for="status in CHART_MODERATION_STATUSES"
          :key="status"
          size="xs"
          :color="actionColor(status)"
          variant="subtle"
          :disabled="selected.size === 0 || overLimit || submitting"
          @click="openConfirm(status)"
        >
          {{ t(`admin.chartActions.${status}`) }}
        </UButton>
      </div>
      <p v-if="overLimit" class="w-full text-xs text-error">
        {{ t('admin.charts.bulkLimit', { count: MAX_CHART_MODERATION_BATCH_SIZE }) }}
      </p>
    </div>

    <UAlert
      v-if="conflicts.size"
      color="warning"
      variant="subtle"
      class="mb-3"
      :title="t('admin.charts.conflictTitle', { count: conflicts.size })"
      :description="t('admin.charts.conflictDescription')"
      close
      @update:open="(open) => !open && bulk.dismissConflicts()"
    />
    <UAlert
      v-if="error"
      color="error"
      :description="translateApiError(error, 'admin.loadFailed')"
    />
    <p v-else-if="pending && !list" class="text-sm text-muted">{{ t('common.loading') }}</p>
    <p v-else-if="!list?.charts.length" class="text-sm text-muted">
      {{ t('admin.users.noCharts') }}
    </p>
    <ul v-else class="divide-y divide-default rounded-lg border border-default">
      <li v-for="chart in list.charts" :key="chart.sha256" class="flex items-start gap-3 px-3 py-2">
        <UCheckbox
          class="mt-1"
          :model-value="bulk.has(chart.sha256)"
          :aria-label="chartLabel(chart)"
          @update:model-value="(value) => toggle(chart, value)"
        />
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-medium">
            <ULink
              v-if="chart.registered"
              :to="localePath(`/charts/${chart.sha256}`)"
              target="_blank"
              class="hover:underline"
            >
              {{ chart.title || t('admin.charts.unregistered') }}
            </ULink>
            <template v-else>{{ t('admin.charts.unregistered') }}</template>
            <span v-if="chart.subtitle" class="text-muted"> {{ chart.subtitle }}</span>
          </p>
          <p class="truncate text-xs text-muted">{{ chart.artist || '-' }}</p>
          <p class="truncate font-mono text-xs text-dimmed">{{ chart.sha256 }}</p>
        </div>
        <div class="shrink-0 text-right text-xs text-muted">
          <AdminModerationStatusBadge kind="chart" :status="chart.status" />
          <p class="mt-1">
            {{
              t('admin.users.chartCounts', {
                user: chart.user_score_count,
                players: chart.player_count,
              })
            }}
          </p>
          <p>{{ formatDateTime(chart.last_played_at) }}</p>
        </div>
      </li>
    </ul>
    <div v-if="list && list.pagination.total > pageSize" class="mt-3 flex justify-end">
      <UPagination v-model:page="page" :items-per-page="pageSize" :total="list.pagination.total" />
    </div>

    <AdminModerationConfirmModal
      v-model:open="confirmOpen"
      :status-label="pendingAction ? t(`admin.chartStatus.${pendingAction.status}`) : ''"
      :targets="pendingAction ? pendingAction.targets.map((target) => target.label) : []"
      :pending="submitting"
      with-note
      @confirm="applyStatus"
    />
  </div>
</template>
