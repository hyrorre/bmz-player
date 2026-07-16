<script setup lang="ts">
interface PlayerListItem {
  id: string
  display_name: string
  bio: string | null
  best_score_count: number
  best_course_score_count: number
  last_score_at: string | null
  updated_at: string
}

interface PlayerListResult {
  players: PlayerListItem[]
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
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { data, pending, error, refresh } = await useFetch<PlayerListResult>('/api/v1/players', {
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

function formatDate(value: string | null): string {
  if (!value) {
    return t('players.noScores')
  }
  return formatDateTime(value)
}

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.playersLoadFailed') : '',
)
useSeoMeta({ title: () => t('players.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('players.title') }}</h1>
        <p class="mt-2 text-sm text-neutral-300">
          {{ t('players.description') }}
        </p>
      </div>

      <div class="mb-6 flex gap-3">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('players.searchPlaceholder')"
          @keydown.enter="applySearch"
        />
        <UButton color="primary" variant="subtle" @click="applySearch">{{
          t('common.search')
        }}</UButton>
      </div>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <p v-else-if="!data?.players.length" class="text-sm text-neutral-400">
        {{ t('players.empty') }}
      </p>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="player in data.players" :key="player.id">
          <NuxtLink
            :to="localePath(`/players/${player.id}`)"
            class="flex items-center justify-between gap-4 px-4 py-3 hover:bg-neutral-900"
          >
            <div class="min-w-0">
              <p class="truncate font-medium">{{ player.display_name }}</p>
              <p class="truncate text-sm text-neutral-400">{{ player.bio }}</p>
            </div>
            <div class="shrink-0 text-right text-sm text-neutral-300">
              <p>
                {{
                  t('players.scoreCounts', {
                    charts: player.best_score_count,
                    courses: player.best_course_score_count,
                  })
                }}
              </p>
              <p class="text-neutral-500">{{ formatDate(player.last_score_at) }}</p>
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
