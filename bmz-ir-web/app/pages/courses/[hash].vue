<script setup lang="ts">
import type { IrRuleMode, LnScorePolicy } from '../../../shared/types/ir'

interface CourseDetail {
  course: {
    course_hash: string
    title: string
    kind: string
    charts: {
      sha256: string
      title: string
      subtitle: string | null
      artist: string | null
      mode: string | null
      level: number | null
      difficulty: string | null
    }[]
    chart_count: number
    constraints: Record<string, unknown>
  }
  stats: { play_count: number }
}

interface CourseRankingEntry {
  rank: number
  player: { id: string; display_name: string }
  score: {
    course_score_id: string
    clear: string
    course_clear: boolean
    ex_score: number
    max_combo: number
    bp: number
    device_type: string
    rule_mode: string
    played_at: string | null
    verification: string
  }
  relation: { is_self: boolean; is_rival: boolean }
}

interface CourseRanking {
  ranking: {
    entries: CourseRankingEntry[]
    self?: {
      rank: number
      score_id: string
      included_in_entries: boolean
      entry?: CourseRankingEntry
    }
  }
}

interface CourseSelfScoresResult {
  scores: {
    course_score_id: string
    clear: string
    course_clear: boolean
    ex_score: number
    max_combo: number
    bp: number
    gauge: string
    ln_policy: string
    rule_mode: string
    device_type: string
    played_at: string | null
    server_received_at: string
    verification: string
  }[]
  pagination: {
    limit: number
    offset: number
    total: number
    has_more: boolean
  }
}

const route = useRoute()
const { user } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const courseHash = computed(() => String(route.params.hash ?? ''))

type CourseGaugeFilter = 'ALL' | 'Class' | 'ExClass' | 'ExHardClass' | 'Normal' | 'Hard'
type CourseLnPolicyFilter = 'ALL' | LnScorePolicy
type CourseRuleModeFilter = 'ALL' | IrRuleMode

const gauge = ref<CourseGaugeFilter>('ALL')
const lnPolicy = ref<CourseLnPolicyFilter>('ALL')
const ruleMode = ref<CourseRuleModeFilter>('ALL')
const gauges: CourseGaugeFilter[] = ['ALL', 'Class', 'ExClass', 'ExHardClass', 'Normal', 'Hard']
const lnPolicies: CourseLnPolicyFilter[] = [
  'ALL',
  'AutoLn',
  'AutoCn',
  'AutoHcn',
  'ForceLn',
  'ForceCn',
  'ForceHcn',
]
const ruleModes: CourseRuleModeFilter[] = ['ALL', 'Beatoraja', 'Lr2Oraja', 'Dx']

const { data: detail, error: detailError } = await useFetch<CourseDetail>(
  () => `/api/v1/courses/${courseHash.value}`,
)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<CourseRanking>(() => `/api/v1/courses/${courseHash.value}/ranking`, {
  query: computed(() => ({
    ...(gauge.value === 'ALL' ? {} : { gauge: gauge.value }),
    ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
    ...(ruleMode.value === 'ALL' ? {} : { rule_mode: ruleMode.value }),
  })),
})

const selfBestEntry = computed<CourseRankingEntry | null>(
  () =>
    ranking.value?.ranking.self?.entry ??
    ranking.value?.ranking.entries.find((entry) => entry.relation.is_self) ??
    null,
)
const canShowSelfArea = computed(() => Boolean(user.value || selfBestEntry.value))
const historyOpen = ref(false)
const historyPage = ref(1)
const historyLimit = 50
const historyOffset = computed(() => (historyPage.value - 1) * historyLimit)
const historyQuery = computed(() => ({
  limit: historyLimit,
  offset: historyOffset.value,
  ...(gauge.value === 'ALL' ? {} : { gauge: gauge.value }),
  ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
  ...(ruleMode.value === 'ALL' ? {} : { rule_mode: ruleMode.value }),
}))
const {
  data: selfHistory,
  pending: selfHistoryPending,
  error: selfHistoryError,
  refresh: refreshSelfHistory,
} = await useFetch<CourseSelfScoresResult>(
  () => `/api/v1/courses/${courseHash.value}/self-scores`,
  {
    immediate: false,
    watch: false,
    query: historyQuery,
  },
)

async function openHistory() {
  historyOpen.value = true
  await refreshSelfHistory()
}

watch([gauge, lnPolicy, ruleMode, courseHash], () => {
  if (!historyOpen.value) {
    historyPage.value = 1
    return
  }
  if (historyPage.value === 1) {
    refreshSelfHistory()
  } else {
    historyPage.value = 1
  }
})

watch(historyPage, () => {
  if (historyOpen.value) {
    refreshSelfHistory()
  }
})

function chartTitle(chart: CourseDetail['course']['charts'][number]): string {
  return chart.title || chart.sha256.slice(0, 16)
}

function chartMeta(chart: CourseDetail['course']['charts'][number]): string {
  return [
    chart.artist,
    chart.mode,
    chart.level == null ? null : `☆${chart.level}`,
    chart.difficulty,
  ]
    .filter(Boolean)
    .join(' / ')
}

function formatScoreDate(value: string | null) {
  return value ? formatDateTime(value) : '-'
}

const detailErrorDescription = computed(() =>
  detailError.value ? translateApiError(detailError.value, 'errors.courseLoadFailed') : '',
)
const rankingErrorDescription = computed(() =>
  rankingError.value ? translateApiError(rankingError.value, 'errors.rankingLoadFailed') : '',
)
const historyErrorDescription = computed(() =>
  selfHistoryError.value
    ? translateApiError(selfHistoryError.value, 'errors.historyLoadFailed')
    : '',
)
useSeoMeta({ title: () => detail.value?.course.title || t('course.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="detailError" color="error" :description="detailErrorDescription" class="mb-6" />
      <template v-else-if="detail">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="localePath('/courses')" class="hover:underline">{{
              t('courses.title')
            }}</NuxtLink>
          </p>
          <h1 class="text-3xl font-semibold">
            {{ detail.course.title || t('common.untitled') }}
            <UBadge
              :color="detail.course.kind === 'dan' ? 'warning' : 'neutral'"
              size="md"
              variant="subtle"
            >
              {{ detail.course.kind === 'dan' ? t('courses.dan') : t('courses.course') }}
            </UBadge>
          </h1>
          <p class="mt-2 text-sm text-neutral-400">
            {{
              t('course.summary', {
                charts: detail.course.chart_count,
                plays: detail.stats.play_count,
              })
            }}
          </p>
        </div>

        <h2 class="mb-2 text-lg font-medium">{{ t('course.charts') }}</h2>
        <ol class="mb-8 list-inside list-decimal space-y-1 text-sm">
          <li v-for="chart in detail.course.charts" :key="chart.sha256">
            <NuxtLink :to="localePath(`/charts/${chart.sha256}`)" class="hover:underline">
              {{ chartTitle(chart) }}
              <span v-if="chart.subtitle" class="text-neutral-400">{{ chart.subtitle }}</span>
            </NuxtLink>
            <span v-if="chartMeta(chart)" class="ml-2 text-neutral-500">
              {{ chartMeta(chart) }}
            </span>
          </li>
        </ol>

        <div class="mb-4 flex flex-wrap gap-3">
          <USelect v-model="gauge" :items="gauges" class="w-44" />
          <USelect v-model="lnPolicy" :items="lnPolicies" class="w-40" />
          <USelect v-model="ruleMode" :items="ruleModes" class="w-40" />
        </div>

        <div
          v-if="canShowSelfArea"
          class="mb-4 flex flex-col gap-3 rounded-lg border border-neutral-800 p-4 sm:flex-row sm:items-center sm:justify-between"
        >
          <div v-if="selfBestEntry" class="min-w-0">
            <p class="text-xs text-neutral-500">{{ t('ranking.personalBest') }}</p>
            <div class="mt-1 flex flex-wrap items-baseline gap-x-4 gap-y-1">
              <p class="text-sm text-neutral-300">#{{ selfBestEntry.rank }}</p>
              <p class="text-xl font-semibold">EX {{ selfBestEntry.score.ex_score }}</p>
              <p class="text-sm">CLEAR {{ selfBestEntry.score.clear }}</p>
              <p class="text-sm">COMBO {{ selfBestEntry.score.max_combo }}</p>
              <p class="text-sm">BP {{ selfBestEntry.score.bp }}</p>
            </div>
            <p class="mt-1 text-xs text-neutral-500">
              {{ selfBestEntry.score.rule_mode }} / {{ selfBestEntry.score.device_type }}
            </p>
          </div>
          <p v-else class="text-sm text-neutral-400">{{ t('ranking.noPersonalBest') }}</p>
          <UButton
            icon="i-lucide-list"
            color="neutral"
            variant="subtle"
            class="shrink-0"
            @click="openHistory"
          >
            {{ t('ranking.selfHistory') }}
          </UButton>
        </div>

        <UAlert v-if="rankingError" color="error" :description="rankingErrorDescription" />
        <p v-else-if="rankingPending" class="text-sm text-neutral-400">
          {{ t('ranking.loading') }}
        </p>
        <p v-else-if="!ranking?.ranking.entries.length" class="text-sm text-neutral-400">
          {{ t('ranking.noScores') }}
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-neutral-800">
          <table class="w-full text-sm">
            <thead class="bg-neutral-900 text-left text-neutral-300">
              <tr>
                <th class="px-3 py-2">#</th>
                <th class="px-3 py-2">{{ t('table.player') }}</th>
                <th class="px-3 py-2 text-right">EX</th>
                <th class="px-3 py-2">{{ t('table.clear') }}</th>
                <th class="px-3 py-2 text-right">COMBO</th>
                <th class="px-3 py-2 text-right">BP</th>
                <th class="px-3 py-2">{{ t('table.date') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="entry in ranking.ranking.entries"
                :key="entry.score.course_score_id"
                class="border-t border-neutral-800"
              >
                <td class="px-3 py-2 text-neutral-300">{{ entry.rank }}</td>
                <td class="px-3 py-2">
                  <NuxtLink :to="localePath(`/players/${entry.player.id}`)" class="hover:underline">
                    {{ entry.player.display_name }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2 text-right font-medium">{{ entry.score.ex_score }}</td>
                <td class="px-3 py-2">
                  {{ entry.score.clear }}
                  <UBadge
                    v-if="entry.score.course_clear"
                    color="success"
                    size="sm"
                    variant="subtle"
                  >
                    CLEAR
                  </UBadge>
                </td>
                <td class="px-3 py-2 text-right">{{ entry.score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.bp }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ formatScoreDate(entry.score.played_at) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <UModal v-model:open="historyOpen" :title="t('ranking.selfHistory')">
          <template #body>
            <UAlert
              v-if="selfHistoryError"
              color="error"
              :description="historyErrorDescription"
              class="mb-4"
            />
            <p v-else-if="selfHistoryPending" class="text-sm text-neutral-400">
              {{ t('common.loading') }}
            </p>
            <p v-else-if="!selfHistory?.scores.length" class="text-sm text-neutral-400">
              {{ t('ranking.noHistory') }}
            </p>
            <div v-else class="overflow-x-auto rounded-lg border border-neutral-800">
              <table class="w-full text-sm">
                <thead class="bg-neutral-900 text-left text-neutral-300">
                  <tr>
                    <th class="px-3 py-2">{{ t('table.date') }}</th>
                    <th class="px-3 py-2 text-right">EX</th>
                    <th class="px-3 py-2">{{ t('table.clear') }}</th>
                    <th class="px-3 py-2 text-right">COMBO</th>
                    <th class="px-3 py-2 text-right">BP</th>
                    <th class="px-3 py-2">{{ t('table.conditions') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="score in selfHistory.scores"
                    :key="score.course_score_id"
                    class="border-t border-neutral-800"
                  >
                    <td class="px-3 py-2 text-neutral-400">
                      {{ formatScoreDate(score.played_at ?? score.server_received_at) }}
                    </td>
                    <td class="px-3 py-2 text-right font-medium">{{ score.ex_score }}</td>
                    <td class="px-3 py-2">{{ score.clear }}</td>
                    <td class="px-3 py-2 text-right">{{ score.max_combo }}</td>
                    <td class="px-3 py-2 text-right">{{ score.bp }}</td>
                    <td class="px-3 py-2 text-neutral-400">
                      {{ score.gauge }} / {{ score.ln_policy }} / {{ score.rule_mode }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div
              v-if="selfHistory && selfHistory.pagination.total > historyLimit"
              class="mt-4 flex justify-end"
            >
              <UPagination
                v-model:page="historyPage"
                :items-per-page="historyLimit"
                :total="selfHistory.pagination.total"
              />
            </div>
          </template>
        </UModal>
      </template>
    </section>
  </main>
</template>
