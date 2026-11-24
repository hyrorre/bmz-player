<script setup lang="ts">
import { NuxtLink } from '#components'
import {
  IR_CLEAR_LABELS,
  IR_DISTRIBUTION_PAGE_SIZE,
  IR_SCORE_RANK_LABELS,
} from '../../shared/constants/ir'
import type {
  IrDifficultyDistribution,
  IrDistributionCharts,
  IrDistributionMode,
} from '../../shared/types/ir'

const route = useRoute()
const localePath = useLocalePath()
const { user } = useUserSession()
const { t } = useI18n()
const { formatNumber } = useLocaleFormat()
const { translateApiError } = useApiError()
const requestFetch = useRequestFetch()
const preferenceCookieOptions = {
  maxAge: 60 * 60 * 24 * 365,
  sameSite: 'lax',
} as const
const preferredTable = useCookie<string>('ir-analysis-table', preferenceCookieOptions)
const preferredMode = useCookie<IrDistributionMode>('ir-analysis-mode', preferenceCookieOptions)

function queryText(value: unknown) {
  return typeof value === 'string' ? value.trim() || undefined : undefined
}

const playerId = computed(() => queryText(route.query.player) ?? user.value?.id)
const tableId = computed(() => queryText(route.query.table) ?? queryText(preferredTable.value))
const mode = computed<IrDistributionMode>(() => {
  const { mode, level, rank } = route.query
  if (mode === 'lamp' || mode === 'score') return mode
  // Older chart list links omit the clear lamp mode.
  if (typeof level === 'string' && typeof rank === 'string') return 'lamp'
  return preferredMode.value === 'score' ? 'score' : 'lamp'
})
const labels = computed(() => (mode.value === 'score' ? IR_SCORE_RANK_LABELS : IR_CLEAR_LABELS))
const modeItems = computed(() => [
  { label: t('analysis.clearMode'), value: 'lamp' },
  { label: t('analysis.scoreMode'), value: 'score' },
])
const reportQuery = computed(() => ({
  player: playerId.value,
  table: tableId.value,
  mode: mode.value,
  include: 'first_pages',
}))
const { data, status, error } = await useAsyncData(
  () => `analysis:${JSON.stringify(reportQuery.value)}`,
  async (_nuxtApp, { signal }) => ({
    report: reportQuery.value.player
      ? await requestFetch<IrDifficultyDistribution>('/api/v1/analysis', {
          query: reportQuery.value,
          signal,
        })
      : null,
  }),
)
const report = computed(() => data.value?.report)
const tableItems = computed(() =>
  (report.value?.tables ?? []).map(({ id, name, symbol }) => ({
    label: symbol ? `${name} (${symbol})` : name,
    value: id,
  })),
)
const total = computed(() => report.value?.levels.reduce((sum, level) => sum + level.total, 0) ?? 0)
const legend = computed(() => labels.value.map((label, rank) => ({ label, rank })).reverse())

async function setView(query: { table?: string; mode?: IrDistributionMode }) {
  const failure = await navigateTo({
    path: route.path,
    query: {
      ...route.query,
      table: tableId.value,
      mode: mode.value,
      ...query,
      level: undefined,
      rank: undefined,
      chart_page: undefined,
    },
  })
  if (failure) return
  if (query.table) preferredTable.value = query.table
  if (query.mode) preferredMode.value = query.mode
}

const selection = computed(() => {
  const table = report.value?.table
  const { level, rank } = route.query
  if (!table || typeof level !== 'string' || typeof rank !== 'string') return
  const segment = report.value?.levels
    .find((item) => item.level === level)
    ?.segments.find((item) => String(item.rank) === rank)
  return segment ? { level, ...segment, table } : undefined
})
const modalOpen = computed({
  get: () => Boolean(selection.value),
  set(open) {
    if (open) return
    void navigateTo(
      {
        path: route.path,
        query: {
          ...route.query,
          mode: mode.value,
          level: undefined,
          rank: undefined,
          chart_page: undefined,
        },
      },
      { replace: true },
    )
  },
})
let segmentButton: HTMLButtonElement | null = null

function showCharts(level: string, rank: number, event: MouseEvent) {
  segmentButton = event.currentTarget as HTMLButtonElement
  void navigateTo({
    path: route.path,
    query: {
      ...route.query,
      table: tableId.value,
      mode: mode.value,
      level,
      rank: String(rank),
      chart_page: undefined,
    },
  })
}

function restoreSegmentFocus() {
  if (segmentButton?.isConnected) segmentButton.focus()
  segmentButton = null
}

const { page, pageSize, offset, pageLocation } = useRoutePagination({
  pageSize: IR_DISTRIBUTION_PAGE_SIZE,
  queryKey: 'chart_page',
  maxOffset: 100_000,
})
const firstPage = computed(() => (offset.value === 0 ? selection.value?.first_page : undefined))
const chartsQuery = computed(() =>
  selection.value && firstPage.value === undefined
    ? {
        table: selection.value.table.id,
        level: selection.value.level,
        rank: selection.value.rank,
        player: playerId.value,
        mode: mode.value,
        limit: pageSize,
        offset: offset.value,
      }
    : null,
)
const {
  data: chartResult,
  status: chartsStatus,
  error: chartsError,
} = await useAsyncData(
  () => `analysis-charts:${JSON.stringify(chartsQuery.value)}`,
  async (_nuxtApp, { signal }) => ({
    result: chartsQuery.value
      ? await requestFetch<IrDistributionCharts>('/api/v1/analysis/charts', {
          query: chartsQuery.value,
          signal,
        })
      : null,
  }),
  { lazy: true },
)
const chartList = computed<IrDistributionCharts | null | undefined>(() => {
  if (!selection.value || firstPage.value === undefined) return chartResult.value?.result
  return {
    charts: firstPage.value,
    pagination: {
      limit: pageSize,
      offset: 0,
      total: selection.value.count,
      has_more: selection.value.count > pageSize,
    },
  }
})
const maxChartPage = computed(() =>
  Math.max(1, Math.ceil((chartList.value?.pagination.total ?? 0) / pageSize)),
)
const modalTitle = computed(() =>
  selection.value
    ? `${selection.value.table.symbol}${selection.value.level} · ${labels.value[selection.value.rank]}`
    : '',
)
const modalCount = computed(() => chartList.value?.pagination.total ?? selection.value?.count ?? 0)

useSeoMeta({ title: () => t('analysis.title') })
</script>

<template>
  <main class="min-w-0">
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
      <header class="mb-6">
        <h1 class="text-3xl font-semibold tracking-tight text-highlighted">
          {{ t('analysis.title') }}
        </h1>
        <p v-if="playerId" class="mt-2 text-sm text-muted">{{ t('analysis.description') }}</p>
      </header>

      <div v-if="!playerId" class="space-y-4 rounded-lg border border-default bg-elevated/40 p-5">
        <p class="text-sm text-muted">{{ t('analysis.choosePlayer') }}</p>
        <div class="flex flex-wrap gap-2">
          <UButton :to="localePath('/players')" icon="i-lucide-users">{{
            t('rivals.findPlayers')
          }}</UButton>
          <UButton :to="localePath('/login')" color="neutral" variant="outline">{{
            t('nav.login')
          }}</UButton>
        </div>
      </div>
      <p v-else-if="status === 'pending'" class="text-sm text-muted">{{ t('common.loading') }}</p>
      <UAlert
        v-else-if="error"
        color="error"
        :description="translateApiError(error, 'errors.analysisLoadFailed')"
      />
      <template v-else-if="report">
        <NuxtLink
          :to="localePath(`/players/${report.player.id}`)"
          class="mb-4 inline-flex items-center gap-2 text-sm font-medium text-primary hover:underline"
        >
          <UIcon name="i-lucide-user" class="size-4" />
          {{ report.player.display_name || report.player.id }}
        </NuxtLink>
        <p v-if="!report.table" class="text-sm text-muted">{{ t('analysis.noTables') }}</p>
        <template v-else>
          <div class="mb-6 flex flex-wrap items-end justify-between gap-4">
            <UFormField :label="t('analysis.table')">
              <USelectMenu
                :model-value="report.table.id"
                :items="tableItems"
                value-key="value"
                class="w-full sm:w-76"
                @update:model-value="(table) => setView({ table })"
              />
            </UFormField>
            <UTabs
              :items="modeItems"
              :content="false"
              :model-value="mode"
              :aria-label="t('analysis.mode')"
              @update:model-value="
                (value) => setView({ mode: value === 'score' ? 'score' : 'lamp' })
              "
            />
          </div>

          <p
            v-if="!report.levels.length"
            class="rounded-lg border border-default p-5 text-sm text-muted"
          >
            {{ t('analysis.noCharts') }}
          </p>
          <div v-else class="rounded-xl border border-default bg-default">
            <div class="border-b border-default px-4 py-4 sm:px-5">
              <div class="flex flex-wrap items-baseline justify-between gap-2">
                <h2 class="font-semibold text-highlighted">{{ report.table.name }}</h2>
                <p class="text-sm text-muted">
                  {{ t('analysis.chartCount', { count: formatNumber(total) }) }}
                </p>
              </div>
              <ul class="mt-3 flex flex-wrap gap-x-3 gap-y-2 text-xs text-muted">
                <li
                  v-for="item in legend"
                  :key="item.rank"
                  class="inline-flex items-center gap-1.5"
                >
                  <span class="rank-swatch" :data-rank="item.rank" />
                  {{ item.label }}
                </li>
              </ul>
            </div>

            <ul class="space-y-2 px-3 py-4 sm:px-5">
              <li v-for="level in report.levels" :key="level.level" class="distribution-row">
                <span
                  class="text-right text-sm font-semibold wrap-anywhere tabular-nums text-highlighted"
                >
                  {{ report.table.symbol }}{{ level.level }}
                </span>
                <div
                  class="distribution-bar"
                  role="group"
                  :aria-label="`${report.table.symbol}${level.level}`"
                >
                  <button
                    v-for="segment in level.segments"
                    :key="segment.rank"
                    type="button"
                    class="rank-segment"
                    :data-rank="segment.rank"
                    :style="{ '--rank-count': segment.count }"
                    :title="`${labels[segment.rank]} · ${formatNumber(segment.count)}`"
                    :aria-label="
                      t('analysis.segment', {
                        level: `${report.table.symbol}${level.level}`,
                        rank: labels[segment.rank],
                        count: formatNumber(segment.count),
                      })
                    "
                    aria-haspopup="dialog"
                    @click="showCharts(level.level, segment.rank, $event)"
                  >
                    {{ formatNumber(segment.count) }}
                  </button>
                </div>
                <span class="text-right text-xs tabular-nums text-muted">{{
                  formatNumber(level.total)
                }}</span>
              </li>
            </ul>
          </div>
        </template>
      </template>

      <UModal
        v-model:open="modalOpen"
        :title="modalTitle"
        :description="t('analysis.chartCount', { count: formatNumber(modalCount) })"
        :ui="{ content: 'sm:max-w-2xl', body: 'p-0 sm:p-0' }"
        @after:leave="restoreSegmentFocus"
      >
        <template #body>
          <p v-if="chartsQuery && chartsStatus === 'pending'" class="p-5 text-sm text-muted">
            {{ t('common.loading') }}
          </p>
          <UAlert
            v-else-if="chartsQuery && chartsError"
            class="m-4"
            color="error"
            :description="translateApiError(chartsError, 'errors.chartsLoadFailed')"
          />
          <p v-else-if="!chartList?.charts.length" class="p-5 text-sm text-muted">
            {{ t('analysis.noCharts') }}
          </p>
          <ul v-else class="divide-y divide-default">
            <li v-for="chart in chartList.charts" :key="chart.id">
              <component
                :is="chart.sha256 ? NuxtLink : 'div'"
                :to="chart.sha256 ? localePath(`/charts/${chart.sha256}`) : undefined"
                class="flex items-center justify-between gap-3 px-5 py-3"
                :class="
                  chart.sha256
                    ? 'hover:bg-elevated focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-primary'
                    : undefined
                "
              >
                <span class="min-w-0">
                  <span class="block font-medium wrap-anywhere text-highlighted">
                    {{ chart.title || chart.sha256?.slice(0, 12) }}
                    <span v-if="chart.subtitle" class="font-normal text-muted">{{
                      chart.subtitle
                    }}</span>
                  </span>
                  <span v-if="chart.artist" class="block text-sm wrap-anywhere text-muted">{{
                    chart.artist
                  }}</span>
                </span>
                <UIcon
                  v-if="chart.sha256"
                  name="i-lucide-chevron-right"
                  class="size-4 shrink-0 text-muted"
                />
                <UBadge v-else color="neutral" variant="subtle" size="sm" class="shrink-0">{{
                  t('analysis.unregistered')
                }}</UBadge>
              </component>
            </li>
          </ul>
        </template>
        <template v-if="chartList && (maxChartPage > 1 || page > 1)" #footer>
          <UPagination
            :page="Math.min(page, maxChartPage)"
            :items-per-page="pageSize"
            :total="Math.max(1, chartList.pagination.total)"
            :to="pageLocation"
            :sibling-count="1"
            :show-controls="maxChartPage > 1"
            size="sm"
          />
        </template>
      </UModal>
    </section>
  </main>
</template>

<style scoped>
.distribution-row {
  display: grid;
  grid-template-columns: 3.25rem minmax(0, 1fr) 4ch;
  gap: 0.5rem;
  align-items: center;
}

.distribution-bar {
  display: flex;
  overflow: hidden;
  min-height: 2.125rem;
  border-radius: 0.45rem;
  background: var(--ui-bg-elevated);
}

.rank-swatch {
  inline-size: 0.65rem;
  block-size: 0.65rem;
  border-radius: 0.2rem;
  background: var(--ir-rank-fill);
}

.rank-segment {
  flex: var(--rank-count) 1 0%;
  min-inline-size: 1.25rem;
  overflow: hidden;
  cursor: pointer;
  background: var(--ir-rank-fill);
  color: var(--ir-rank-text);
  font-size: 0.75rem;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
}

.rank-segment:hover,
.rank-segment:focus-visible {
  outline: 2px solid currentColor;
  outline-offset: -2px;
  z-index: 1;
}

@media (min-width: 640px) {
  .distribution-row {
    grid-template-columns: 4.5rem minmax(0, 1fr) 5ch;
    gap: 0.75rem;
  }
}
</style>
