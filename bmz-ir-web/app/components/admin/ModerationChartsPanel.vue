<script setup lang="ts">
import {
  CHART_MODERATION_STATUSES,
  MAX_CHART_MODERATION_BATCH_SIZE,
  type ChartModerationStatus,
} from '#shared/constants/moderation'
import type {
  AdminChartDetail,
  AdminChartListItem,
  ModerationChangeResult,
  Pagination,
} from '../../types/moderation'

type SearchField = 'any' | 'sha256' | 'md5' | 'title' | 'artist'

const { t } = useI18n()
const localePath = useLocalePath()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { notifyResults, notifyError } = useModerationResultToast()

const search = ref('')
const appliedSearch = ref('')
const field = ref<SearchField>('any')
const statusFilter = ref<'all' | ChartModerationStatus>('all')
const page = ref(1)
const pageSize = 50
// 一括操作の対象。ページを移動しても、選択した時点の revision を保持する。
const bulk = useBulkChartSelection()
const { selected, conflicts } = bulk

const fieldItems = computed(() =>
  (['any', 'sha256', 'md5', 'title', 'artist'] as const).map((value) => ({
    label: t(`admin.charts.field.${value}`),
    value,
  })),
)
const statusItems = computed(() => [
  { label: t('admin.allStatuses'), value: 'all' as const },
  ...CHART_MODERATION_STATUSES.map((status) => ({
    label: t(`admin.chartStatus.${status}`),
    value: status,
  })),
])

const {
  data: list,
  pending: listPending,
  error: listError,
  refresh: refreshList,
} = await useFetch<{ charts: AdminChartListItem[]; pagination: Pagination }>(
  '/api/v1/admin/moderation/charts',
  {
    query: computed(() => ({
      limit: pageSize,
      offset: (page.value - 1) * pageSize,
      field: field.value,
      ...(appliedSearch.value ? { q: appliedSearch.value } : {}),
      ...(statusFilter.value === 'all' ? {} : { status: statusFilter.value }),
    })),
    watch: [page, statusFilter],
  },
)

const {
  selectedId: detailSha,
  detail,
  loading: detailLoading,
  error: detailError,
  select: selectChart,
  reload: reloadDetail,
} = useModerationTarget(
  (sha256) => $fetch<AdminChartDetail>(`/api/v1/admin/moderation/charts/${sha256}`),
  (value) => value.sha256,
)

watch(statusFilter, () => {
  page.value = 1
})

function applySearch() {
  appliedSearch.value = search.value.trim()
  if (page.value === 1) refreshList()
  else page.value = 1
}

function chartLabel(chart: { title: string; sha256: string }) {
  return `${chart.title || t('admin.charts.unregistered')} (${chart.sha256.slice(0, 12)}…)`
}

function toggle(chart: AdminChartListItem, value: boolean | 'indeterminate') {
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

/** 確認ダイアログで確定する操作。開いた時点の対象と revision を固定し、送信まで変えない。 */
interface PendingChartAction {
  readonly status: ChartModerationStatus
  readonly targets: ReadonlyArray<Readonly<{ sha256: string; label: string; revision: number }>>
  readonly initialNote: string
  readonly fromSelection: boolean
}

const confirmOpen = ref(false)
const pendingAction = shallowRef<PendingChartAction | null>(null)
const submitting = ref(false)
const detailActionsDisabled = computed(
  () => detailLoading.value || submitting.value || !detail.value,
)

function openBulkConfirm(status: ChartModerationStatus) {
  if (submitting.value || selected.value.size === 0 || overLimit.value) return
  pendingAction.value = Object.freeze({
    status,
    targets: Object.freeze(bulk.snapshot().map((target) => Object.freeze(target))),
    initialNote: '',
    fromSelection: true,
  })
  confirmOpen.value = true
}

function openDetailConfirm(status: ChartModerationStatus) {
  const target = detail.value
  if (!target || detailActionsDisabled.value) return
  pendingAction.value = Object.freeze({
    status,
    targets: Object.freeze([
      Object.freeze({
        sha256: target.sha256,
        label: chartLabel({ title: target.chart?.title ?? '', sha256: target.sha256 }),
        revision: target.moderation.revision,
      }),
    ]),
    initialNote: target.moderation.note,
    fromSelection: false,
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
    // 詳細からの単独操作でも、同じ譜面を一括選択していれば選択の revision は古くなる。
    bulk.applyResults(
      response.results.filter((result) => action.fromSelection || bulk.has(result.sha256)),
      new Map(action.targets.map((target) => [target.sha256, target.label])),
    )
  } catch (error) {
    notifyError(error)
  } finally {
    submitting.value = false
    confirmOpen.value = false
    pendingAction.value = null
    const touchedDetail = action.targets.some((target) => target.sha256 === detailSha.value)
    await Promise.all([refreshList(), touchedDetail ? reloadDetail() : Promise.resolve()])
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

const detailActions = computed(() =>
  detail.value
    ? CHART_MODERATION_STATUSES.filter((status) => status !== detail.value!.moderation.status)
    : [],
)
</script>

<template>
  <div class="grid gap-6 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
    <section class="min-w-0">
      <div class="mb-4 flex flex-col gap-2 lg:flex-row">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('admin.charts.searchPlaceholder')"
          @keydown.enter="applySearch"
        />
        <div class="flex flex-wrap gap-2">
          <USelect v-model="field" :items="fieldItems" class="w-32" />
          <USelect v-model="statusFilter" :items="statusItems" class="w-36" />
          <UButton color="primary" variant="subtle" @click="applySearch">
            {{ t('common.search') }}
          </UButton>
        </div>
      </div>

      <div
        class="sticky top-0 z-10 mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-default bg-default/90 px-3 py-2 backdrop-blur"
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
        <UButton
          v-if="selected.size"
          size="xs"
          color="neutral"
          variant="ghost"
          @click="bulk.clear()"
        >
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
            @click="openBulkConfirm(status)"
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
      >
        <template #actions>
          <div class="flex flex-wrap gap-1">
            <UButton
              v-for="[sha256, entry] in conflicts"
              :key="sha256"
              size="xs"
              color="warning"
              variant="outline"
              @click="selectChart(sha256)"
            >
              {{ entry.label }}
            </UButton>
          </div>
        </template>
      </UAlert>
      <UAlert
        v-if="listError"
        color="error"
        :description="translateApiError(listError, 'admin.loadFailed')"
      />
      <p v-else-if="listPending && !list" class="text-sm text-muted">{{ t('common.loading') }}</p>
      <p v-else-if="!list?.charts.length" class="text-sm text-muted">
        {{ t('admin.charts.empty') }}
      </p>
      <ul v-else class="divide-y divide-default rounded-lg border border-default">
        <li
          v-for="chart in list.charts"
          :key="chart.sha256"
          class="flex items-start gap-3 px-3 py-3"
          :class="{ 'bg-elevated': detailSha === chart.sha256 }"
        >
          <UCheckbox
            class="mt-1"
            :model-value="bulk.has(chart.sha256)"
            :aria-label="chartLabel(chart)"
            @update:model-value="(value) => toggle(chart, value)"
          />
          <button type="button" class="min-w-0 flex-1 text-left" @click="selectChart(chart.sha256)">
            <p class="truncate font-medium">
              {{ chart.title || t('admin.charts.unregistered') }}
              <span v-if="chart.subtitle" class="text-muted">{{ chart.subtitle }}</span>
            </p>
            <p class="truncate text-sm text-muted">{{ chart.artist || '-' }}</p>
            <p class="truncate font-mono text-xs text-dimmed">{{ chart.sha256 }}</p>
            <p v-if="chart.md5" class="truncate font-mono text-xs text-dimmed">
              MD5 {{ chart.md5 }}
            </p>
          </button>
          <div class="shrink-0 text-right text-xs text-muted">
            <AdminModerationStatusBadge kind="chart" :status="chart.status" />
            <p class="mt-1">
              {{
                t('admin.charts.counts', { players: chart.player_count, scores: chart.score_count })
              }}
            </p>
          </div>
        </li>
      </ul>
      <div v-if="list && list.pagination.total > pageSize" class="mt-4 flex justify-end">
        <UPagination
          v-model:page="page"
          :items-per-page="pageSize"
          :total="list.pagination.total"
        />
      </div>
    </section>

    <section class="min-w-0 rounded-xl border border-default p-5">
      <p v-if="!detailSha" class="text-sm text-muted">{{ t('admin.notSelected') }}</p>
      <UAlert
        v-else-if="detailError"
        color="error"
        :description="translateApiError(detailError, 'admin.loadFailed')"
      />
      <p v-else-if="detailLoading" class="text-sm text-muted">
        {{ t('common.loading') }}
      </p>
      <template v-else-if="detail">
        <div class="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div class="min-w-0">
            <h2 class="text-xl font-semibold break-words">
              {{ detail.chart?.title || t('admin.charts.unregistered') }}
            </h2>
            <p class="text-sm text-muted">{{ detail.chart?.artist || '-' }}</p>
            <p class="font-mono text-xs break-all text-dimmed">{{ detailSha }}</p>
            <p v-if="detail.chart?.md5" class="font-mono text-xs break-all text-dimmed">
              MD5 {{ detail.chart.md5 }}
            </p>
          </div>
          <AdminModerationStatusBadge kind="chart" :status="detail.moderation.status" />
        </div>
        <p class="mb-4 text-sm">
          {{
            t('admin.charts.counts', {
              players: detail.stats.player_count,
              scores: detail.stats.score_count,
            })
          }}
        </p>

        <div v-if="detail.moderation.revision > 0" class="mb-4 rounded-lg bg-elevated p-3 text-sm">
          <p>
            <span class="text-muted">{{ t('admin.reason') }}:</span>
            {{ detail.moderation.reason || '-' }}
          </p>
          <p v-if="detail.moderation.note">
            <span class="text-muted">{{ t('admin.charts.note') }}:</span>
            {{ detail.moderation.note }}
          </p>
          <p class="text-muted">
            {{ t('admin.updatedAt') }}:
            {{ detail.moderation.updated_at ? formatDateTime(detail.moderation.updated_at) : '-' }}
            ·
            {{ t('admin.updatedBy') }}:
            {{
              detail.moderation.updated_by?.display_name || detail.moderation.updated_by?.id || '-'
            }}
          </p>
        </div>

        <div class="mb-6 flex flex-wrap gap-2">
          <UButton
            v-for="status in detailActions"
            :key="status"
            :color="actionColor(status)"
            variant="subtle"
            :disabled="detailActionsDisabled"
            @click="openDetailConfirm(status)"
          >
            {{ t(`admin.chartActions.${status}`) }}
          </UButton>
          <UButton
            v-if="detail.chart"
            color="neutral"
            variant="ghost"
            icon="i-lucide-external-link"
            :to="localePath(`/charts/${detailSha}`)"
            target="_blank"
          >
            {{ t('admin.charts.viewChart') }}
          </UButton>
        </div>

        <template v-if="detail.same_md5_charts.length">
          <h3 class="mb-1 font-semibold">{{ t('admin.charts.sameMd5') }}</h3>
          <p class="mb-2 text-xs text-muted">{{ t('admin.charts.sameMd5Note') }}</p>
          <ul class="mb-6 space-y-1 text-sm">
            <li
              v-for="chart in detail.same_md5_charts"
              :key="chart.sha256"
              class="flex items-center justify-between gap-2"
            >
              <button
                type="button"
                class="min-w-0 truncate text-left hover:underline"
                @click="selectChart(chart.sha256)"
              >
                {{ chart.title || chart.sha256 }}
              </button>
              <AdminModerationStatusBadge kind="chart" :status="chart.status" />
            </li>
          </ul>
        </template>

        <h3 class="mb-2 font-semibold">{{ t('admin.charts.players') }}</h3>
        <p v-if="!detail.players.length" class="text-sm text-muted">
          {{ t('admin.charts.noPlayers') }}
        </p>
        <ul v-else class="divide-y divide-default rounded-lg border border-default text-sm">
          <li
            v-for="player in detail.players"
            :key="player.player_id"
            class="flex items-center justify-between gap-2 px-3 py-2"
          >
            <div class="min-w-0">
              <p class="truncate">{{ player.display_name || player.player_id }}</p>
              <p class="text-xs text-muted">
                EX {{ player.best_ex_score }} · {{ player.score_count }} ·
                {{ formatDateTime(player.last_received_at) }}
              </p>
            </div>
            <AdminModerationStatusBadge
              v-if="player.user_status !== 'active'"
              kind="user"
              :status="player.user_status"
            />
          </li>
        </ul>
      </template>
    </section>

    <AdminModerationConfirmModal
      v-model:open="confirmOpen"
      :status-label="pendingAction ? t(`admin.chartStatus.${pendingAction.status}`) : ''"
      :targets="pendingAction ? pendingAction.targets.map((target) => target.label) : []"
      :pending="submitting"
      :initial-note="pendingAction?.initialNote ?? ''"
      with-note
      @confirm="applyStatus"
    />
  </div>
</template>
