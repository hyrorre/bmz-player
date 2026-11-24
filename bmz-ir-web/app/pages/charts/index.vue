<script setup lang="ts">
import type { IrDifficultyLabel, IrDifficultyTableOption } from '../../../shared/types/ir'
import {
  readDifficultyFilter,
  type DifficultyFilter,
} from '../../../shared/utils/difficulty_filter'

interface ChartListItem {
  sha256: string
  title: string
  subtitle: string | null
  genre: string | null
  artist: string | null
  mode: string
  level: number | null
  notes: number
  difficulty_labels: IrDifficultyLabel[]
  updated_at: string
}

interface ChartListResult {
  difficulty_filter: IrDifficultyLabel | null
  charts: ChartListItem[]
  pagination: {
    limit: number
    offset: number
    total: number
    has_more: boolean
  }
}

const { search, page, pageSize, pageLocation, listQuery, apiQuery, applySearch } =
  useSearchableListQuery()
const route = useRoute()
const difficulty = computed(() => readDifficultyFilter(route.query))
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()
const [
  { data, pending, error, refresh },
  { data: catalog, pending: catalogPending, error: catalogError },
] = await Promise.all([
  useFetch<ChartListResult>('/api/v1/charts', {
    query: computed(() => ({ ...apiQuery.value, ...difficulty.value })),
  }),
  useFetch<{ tables: IrDifficultyTableOption[] }>('/api/v1/difficulty-tables'),
])

const selectedTable = computed(() => {
  const tableId = difficulty.value?.difficulty_table
  if (!tableId) return
  return catalog.value?.tables.find(({ id, legacy_id }) => id === tableId || legacy_id === tableId)
})
const selectedTableId = computed(
  () => selectedTable.value?.id ?? difficulty.value?.difficulty_table,
)
const chartListQuery = computed(() => ({
  ...listQuery.value,
  ...difficulty.value,
  difficulty_table: selectedTableId.value,
}))
const matchedLabel = computed(() => {
  const label = data.value?.difficulty_filter
  return label?.table_id === selectedTableId.value ? label : undefined
})

const tableItems = computed(() => {
  const items = [{ value: 'all', label: t('charts.allDifficultyTables') }]
  for (const table of catalog.value?.tables ?? []) {
    items.push({
      value: `table:${table.id}`,
      label: table.symbol ? `${table.name} (${table.symbol})` : table.name,
    })
  }
  if (difficulty.value && !selectedTable.value) {
    items.push({
      value: `table:${difficulty.value.difficulty_table}`,
      label: matchedLabel.value?.table_name || difficulty.value.difficulty_table,
    })
  }
  return items
})
const levelItems = computed(() => {
  const levels = selectedTable.value?.levels ?? []
  const current = difficulty.value?.difficulty_level
  const available =
    current !== undefined && !levels.includes(current) ? [...levels, current] : levels
  const symbol = selectedTable.value?.symbol ?? matchedLabel.value?.symbol ?? ''
  return [
    { value: 'all', label: t('charts.allDifficultyLevels') },
    ...available.map((level) => ({
      value: `level:${level}`,
      label: `${symbol}${level}` || t('common.none'),
    })),
  ]
})

function difficultyLocation(filter?: DifficultyFilter) {
  return {
    path: route.path,
    query: {
      ...route.query,
      difficulty_table: filter?.difficulty_table,
      difficulty_level: filter?.difficulty_level,
      page: undefined,
    },
  }
}

const tableChoice = computed({
  get: () => (selectedTableId.value ? `table:${selectedTableId.value}` : 'all'),
  set(value: string) {
    if (value === tableChoice.value) return
    void navigateTo(
      difficultyLocation(
        value === 'all' ? undefined : { difficulty_table: value.slice('table:'.length) },
      ),
    )
  },
})
const levelChoice = computed({
  get: () =>
    difficulty.value?.difficulty_level === undefined
      ? 'all'
      : `level:${difficulty.value.difficulty_level}`,
  set(value: string) {
    if (!selectedTableId.value || value === levelChoice.value) return
    void navigateTo(
      difficultyLocation({
        difficulty_table: selectedTableId.value,
        difficulty_level: value === 'all' ? undefined : value.slice('level:'.length),
      }),
    )
  },
})

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.chartsLoadFailed') : '',
)
useSeoMeta({ title: () => t('charts.title') })
</script>

<template>
  <main class="min-w-0">
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('charts.title') }}</h1>
        <p class="mt-2 text-sm text-neutral-300">
          {{ t('charts.description') }}
        </p>
      </div>

      <form class="mb-4 flex gap-3" @submit.prevent="applySearch(refresh)">
        <UInput
          v-model="search"
          class="min-w-0 flex-1"
          icon="i-lucide-search"
          :placeholder="t('charts.searchPlaceholder')"
          :aria-label="t('charts.searchPlaceholder')"
        />
        <UButton type="submit" color="primary" variant="subtle">{{ t('common.search') }}</UButton>
      </form>

      <div class="mb-4 flex items-end gap-2 sm:gap-3">
        <UFormField :label="t('analysis.table')" class="min-w-0 flex-[2] sm:max-w-76">
          <USelectMenu
            v-model="tableChoice"
            :items="tableItems"
            value-key="value"
            :loading="catalogPending"
            :disabled="tableItems.length === 1"
            class="w-full"
          />
        </UFormField>
        <UFormField :label="t('charts.difficultyFilter')" class="min-w-0 flex-1 sm:max-w-48">
          <USelect
            v-model="levelChoice"
            :items="levelItems"
            :disabled="levelItems.length === 1"
            class="w-full"
          />
        </UFormField>
        <UButton
          v-if="difficulty"
          :to="difficultyLocation()"
          :aria-label="t('charts.clearDifficultyFilter')"
          :title="t('charts.clearDifficultyFilter')"
          icon="i-lucide-x"
          color="neutral"
          variant="subtle"
        />
      </div>
      <p v-if="catalogError" class="mb-4 text-sm text-muted">
        {{ t('charts.difficultyOptionsUnavailable') }}
      </p>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <div v-else-if="!data?.charts.length" class="text-sm text-muted">
        <p>{{ t(difficulty || listQuery.q ? 'charts.noMatches' : 'charts.empty') }}</p>
        <UPagination
          v-if="page > 1"
          :page="1"
          :to="pageLocation"
          :total="1"
          :show-controls="false"
          class="mt-3"
        />
      </div>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="chart in data.charts" :key="chart.sha256">
          <NuxtLink
            :to="localePath({ path: `/charts/${chart.sha256}`, query: chartListQuery })"
            class="flex items-center justify-between gap-4 px-4 py-3 hover:bg-neutral-900"
          >
            <div class="min-w-0">
              <p class="truncate font-medium">
                {{ chart.title }}
                <span v-if="chart.subtitle" class="text-neutral-400">{{ chart.subtitle }}</span>
              </p>
              <p class="truncate text-sm text-neutral-400">
                {{ chart.artist ?? '' }}<span v-if="chart.genre"> / {{ chart.genre }}</span>
              </p>
            </div>
            <div class="shrink-0 text-right text-sm text-neutral-300">
              <p>
                {{ chart.mode }}<span v-if="chart.level != null"> ☆{{ chart.level }}</span>
              </p>
              <p class="text-neutral-500">{{ t('charts.notes', { count: chart.notes }) }}</p>
            </div>
          </NuxtLink>
          <DifficultyTableBadges
            :labels="chart.difficulty_labels"
            :q="listQuery.q"
            class="-mt-1 px-4 pb-3"
          />
        </li>
      </ul>

      <div
        v-if="data?.charts.length && data.pagination.total > pageSize"
        class="mt-6 flex justify-end"
      >
        <UPagination
          :page="page"
          :to="pageLocation"
          :sibling-count="1"
          :items-per-page="pageSize"
          :total="data.pagination.total"
        />
      </div>
    </section>
  </main>
</template>
