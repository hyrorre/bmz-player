<script setup lang="ts">
import type { IrDifficultyLabel } from '../../../shared/types/ir'

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
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()
const { data, pending, error, refresh } = await useFetch<ChartListResult>('/api/v1/charts', {
  query: apiQuery,
})

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.chartsLoadFailed') : '',
)
useSeoMeta({ title: () => t('charts.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('charts.title') }}</h1>
        <p class="mt-2 text-sm text-neutral-300">
          {{ t('charts.description') }}
        </p>
      </div>

      <form class="mb-6 flex gap-3" @submit.prevent="applySearch(refresh)">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('charts.searchPlaceholder')"
          :aria-label="t('charts.searchPlaceholder')"
        />
        <UButton type="submit" color="primary" variant="subtle">{{ t('common.search') }}</UButton>
      </form>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <div v-else-if="!data?.charts.length" class="text-sm text-muted">
        <p>{{ t('charts.empty') }}</p>
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
            :to="localePath({ path: `/charts/${chart.sha256}`, query: listQuery })"
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
              <DifficultyTableBadges :labels="chart.difficulty_labels" class="mt-1.5" />
            </div>
            <div class="shrink-0 text-right text-sm text-neutral-300">
              <p>
                {{ chart.mode }}<span v-if="chart.level != null"> ☆{{ chart.level }}</span>
              </p>
              <p class="text-neutral-500">{{ t('charts.notes', { count: chart.notes }) }}</p>
            </div>
          </NuxtLink>
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
