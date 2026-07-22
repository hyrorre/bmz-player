<script setup lang="ts">
interface ChartListItem {
  sha256: string
  title: string
  subtitle: string | null
  genre: string | null
  artist: string | null
  mode: string
  level: number | null
  notes: number
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

const search = ref('')
const appliedSearch = ref('')
const page = ref(1)
const pageSize = 50
const offset = computed(() => (page.value - 1) * pageSize)
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()
const { data, pending, error, refresh } = await useFetch<ChartListResult>('/api/v1/charts', {
  query: computed(() => ({
    limit: pageSize,
    offset: offset.value,
    ...(appliedSearch.value ? { q: appliedSearch.value } : {}),
  })),
  watch: false,
})

function applySearch() {
  appliedSearch.value = search.value.trim()
  if (page.value === 1) {
    refresh()
  } else {
    page.value = 1
  }
}

watch(page, () => {
  refresh()
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

      <div class="mb-6 flex gap-3">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('charts.searchPlaceholder')"
          @keydown.enter="applySearch"
        />
        <UButton color="primary" variant="subtle" @click="applySearch">{{
          t('common.search')
        }}</UButton>
      </div>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <p v-else-if="!data?.charts.length" class="text-sm text-neutral-400">
        {{ t('charts.empty') }}
      </p>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="chart in data.charts" :key="chart.sha256">
          <NuxtLink
            :to="localePath(`/charts/${chart.sha256}`)"
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
        </li>
      </ul>

      <div v-if="data && data.pagination.total > pageSize" class="mt-6 flex justify-end">
        <UPagination
          v-model:page="page"
          :items-per-page="pageSize"
          :total="data.pagination.total"
        />
      </div>
    </section>
  </main>
</template>
