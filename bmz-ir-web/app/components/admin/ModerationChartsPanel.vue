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
const detailSha = ref<string | null>(null)
// 一括操作の対象。ページを移動しても選択と表示用の情報を保持する。
const selected = ref(new Map<string, { label: string; revision: number }>())

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
  data: detail,
  pending: detailPending,
  error: detailError,
  refresh: refreshDetail,
} = await useFetch<AdminChartDetail>(
  () => `/api/v1/admin/moderation/charts/${detailSha.value ?? ''}`,
  { immediate: false, watch: false },
)

watch(detailSha, (sha) => {
  if (sha) refreshDetail()
})
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

function isSelected(sha256: string) {
  return selected.value.has(sha256)
}

function toggle(chart: AdminChartListItem, value: boolean | 'indeterminate') {
  const next = new Map(selected.value)
  if (value === true) next.set(chart.sha256, { label: chartLabel(chart), revision: chart.revision })
  else next.delete(chart.sha256)
  selected.value = next
}

const pageAllSelected = computed(
  () => Boolean(list.value?.charts.length) && list.value!.charts.every((c) => isSelected(c.sha256)),
)

function togglePage(value: boolean | 'indeterminate') {
  const next = new Map(selected.value)
  for (const chart of list.value?.charts ?? []) {
    if (value === true)
      next.set(chart.sha256, { label: chartLabel(chart), revision: chart.revision })
    else next.delete(chart.sha256)
  }
  selected.value = next
}

// 一覧を再取得したら、選択中の譜面の revision を最新に合わせる。
watch(list, (value) => {
  if (!value) return
  const next = new Map(selected.value)
  for (const chart of value.charts) {
    const entry = next.get(chart.sha256)
    if (entry) next.set(chart.sha256, { ...entry, revision: chart.revision })
  }
  selected.value = next
})

const overLimit = computed(() => selected.value.size > MAX_CHART_MODERATION_BATCH_SIZE)

const confirmOpen = ref(false)
const confirmStatus = ref<ChartModerationStatus>('blocked')
const confirmTargets = ref<Array<{ sha256: string; label: string; revision: number }>>([])
const confirmInitialNote = ref('')
const submitting = ref(false)

function openBulkConfirm(status: ChartModerationStatus) {
  confirmStatus.value = status
  confirmTargets.value = [...selected.value.entries()].map(([sha256, entry]) => ({
    sha256,
    ...entry,
  }))
  confirmInitialNote.value = ''
  confirmOpen.value = true
}

function openDetailConfirm(status: ChartModerationStatus) {
  if (!detail.value || !detailSha.value) return
  confirmStatus.value = status
  confirmTargets.value = [
    {
      sha256: detailSha.value,
      label: chartLabel({ title: detail.value.chart?.title ?? '', sha256: detailSha.value }),
      revision: detail.value.moderation.revision,
    },
  ]
  confirmInitialNote.value = detail.value.moderation.note
  confirmOpen.value = true
}

async function applyStatus({ reason, note }: { reason: string; note?: string }) {
  submitting.value = true
  try {
    const response = await $fetch<{
      results: Array<ModerationChangeResult<ChartModerationStatus> & { sha256: string }>
    }>('/api/v1/admin/moderation/charts/status', {
      method: 'POST',
      body: {
        items: confirmTargets.value.map((target) => ({
          sha256: target.sha256,
          expected_revision: target.revision,
        })),
        status: confirmStatus.value,
        reason,
        note,
      },
    })
    notifyResults(response.results)
    // 適用済み・同じ状態の譜面は選択から外し、競合した譜面だけ残す。
    const next = new Map(selected.value)
    for (const result of response.results) {
      if (result.result !== 'conflict') next.delete(result.sha256)
    }
    selected.value = next
    confirmOpen.value = false
  } catch (error) {
    notifyError(error)
  } finally {
    submitting.value = false
    await Promise.all([refreshList(), detailSha.value ? refreshDetail() : Promise.resolve()])
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
          @click="selected = new Map()"
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
            :disabled="selected.size === 0 || overLimit"
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
            :model-value="isSelected(chart.sha256)"
            :aria-label="chartLabel(chart)"
            @update:model-value="(value) => toggle(chart, value)"
          />
          <button type="button" class="min-w-0 flex-1 text-left" @click="detailSha = chart.sha256">
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
      <p v-else-if="detailPending && !detail" class="text-sm text-muted">
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
                @click="detailSha = chart.sha256"
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
      :status-label="t(`admin.chartStatus.${confirmStatus}`)"
      :targets="confirmTargets.map((target) => target.label)"
      :pending="submitting"
      :initial-note="confirmInitialNote"
      with-note
      @confirm="applyStatus"
    />
  </div>
</template>
