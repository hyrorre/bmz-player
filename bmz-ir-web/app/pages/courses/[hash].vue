<script setup lang="ts">
import type { IrArrangeOptions, IrChartLnProfile, IrRanking } from '../../../shared/types/ir'

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
    ln_profile: IrChartLnProfile & { has_ln: boolean }
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
    max_ex_score: number
    max_combo: number
    bp: number
    cb: number | null
    device_type: string
    ln_policy: string
    rule_mode: string
    arrange_options: (IrArrangeOptions | null)[]
    played_at: string | null
    verification: string
  }
  relation: { is_self: boolean; is_rival: boolean }
}

interface CourseRanking {
  ranking: {
    entries: CourseRankingEntry[]
    pagination: IrRanking['ranking']['pagination']
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
    max_ex_score: number
    max_combo: number
    bp: number
    cb: number | null
    gauge: string
    ln_policy: string
    rule_mode: string
    arrange_options: (IrArrangeOptions | null)[]
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
const listLocation = computed(() =>
  localePath({ path: '/courses', query: listReturnQuery(route.query) }),
)
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const courseHash = computed(() => String(route.params.hash ?? ''))

const { data: detail, error: detailError } = await useFetch<CourseDetail>(
  () => `/api/v1/courses/${courseHash.value}`,
)
const lnPolicies = computed(() => lnFilterItems(detail.value?.course.ln_profile))
const { lnPolicy, ruleMode, ruleQuery, rankingQuery, page, pageSize, maxPage, pageLocation } =
  useRankingQuery(lnPolicies)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<CourseRanking>(() => `/api/v1/courses/${courseHash.value}/ranking`, {
  query: rankingQuery,
})

const selfBestEntry = computed<CourseRankingEntry | null>(
  () =>
    ranking.value?.ranking.self?.entry ??
    ranking.value?.ranking.entries.find((entry) => entry.relation.is_self) ??
    null,
)
const canShowSelfArea = computed(() => Boolean(user.value || selfBestEntry.value))
const {
  open: historyOpen,
  page: historyPage,
  limit: historyLimit,
  data: selfHistory,
  pending: selfHistoryPending,
  error: selfHistoryError,
  openHistory,
} = useSelfScoreHistory<CourseSelfScoresResult>(
  () => `/api/v1/courses/${courseHash.value}/self-scores`,
  () => ruleQuery.value,
)

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

function scoreOption(score: { arrange_options: (IrArrangeOptions | null)[] }) {
  return formatCourseArrange(score.arrange_options, detail.value?.course.charts ?? [])
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
  <main class="min-w-0">
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
      <UAlert v-if="detailError" color="error" :description="detailErrorDescription" class="mb-6" />
      <template v-else-if="detail">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="listLocation" class="hover:underline">{{ t('courses.title') }}</NuxtLink>
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
          <li v-for="(chart, index) in detail.course.charts" :key="index">
            <NuxtLink :to="localePath(`/charts/${chart.sha256}`)" class="hover:underline">
              {{ chartTitle(chart) }}
              <span v-if="chart.subtitle" class="text-neutral-400">{{ chart.subtitle }}</span>
            </NuxtLink>
            <span v-if="chartMeta(chart)" class="ml-2 text-neutral-500">
              {{ chartMeta(chart) }}
            </span>
          </li>
        </ol>

        <div class="mb-4 flex flex-wrap items-center gap-3">
          <UFormField v-if="lnPolicies.length" label="LN" orientation="horizontal">
            <USelect v-model="lnPolicy" :items="lnPolicies" class="w-28" />
          </UFormField>
          <UFormField label="RULE" orientation="horizontal">
            <USelect v-model="ruleMode" :items="ruleModeItems" class="w-32" />
          </UFormField>
        </div>

        <div
          v-if="canShowSelfArea"
          class="mb-4 flex flex-col gap-3 rounded-lg border border-neutral-800 p-4 sm:flex-row sm:items-center sm:justify-between"
        >
          <div v-if="selfBestEntry" class="min-w-0">
            <p class="text-xs text-neutral-500">{{ t('ranking.personalBest') }}</p>
            <div class="mt-1 flex flex-wrap items-baseline gap-x-4 gap-y-1">
              <p class="text-sm text-neutral-300">#{{ selfBestEntry.rank }}</p>
              <RankingExScore
                :score="selfBestEntry.score.ex_score"
                :max="selfBestEntry.score.max_ex_score"
              />
              <RankingScoreRank
                :score="selfBestEntry.score.ex_score"
                :max="selfBestEntry.score.max_ex_score"
              />
              <IrClearBadge :clear="selfBestEntry.score.clear" />
              <p class="text-sm">
                MAX COMBO
                <RankingMaxCombo
                  :combo="selfBestEntry.score.max_combo"
                  :total-notes="totalNotesFromMaxExScore(selfBestEntry.score.max_ex_score)"
                />
              </p>
              <p class="text-sm">BP {{ selfBestEntry.score.bp }}</p>
              <p class="text-sm">CB {{ selfBestEntry.score.cb ?? '-' }}</p>
            </div>
            <p class="mt-1 text-xs text-neutral-500">
              <template v-if="lnPolicies.length"
                >{{ lnPolicyLabel(selfBestEntry.score.ln_policy) }} ·</template
              >
              {{ ruleModeLabel(selfBestEntry.score.rule_mode) }} ·
              {{ scoreOption(selfBestEntry.score) }} · {{ selfBestEntry.score.device_type }}
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

        <RankingPagination
          v-if="ranking && !rankingPending && !rankingError"
          :page="page"
          :page-size="pageSize"
          :max-page="maxPage"
          :total="ranking.ranking.pagination.total"
          :page-location="pageLocation"
        />

        <UAlert v-if="rankingError" color="error" :description="rankingErrorDescription" />
        <p v-else-if="rankingPending" class="text-sm text-neutral-400">
          {{ t('ranking.loading') }}
        </p>
        <p v-else-if="!ranking?.ranking.entries.length" class="text-sm text-neutral-400">
          {{ t('ranking.noScores') }}
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-default">
          <table class="w-full text-sm">
            <thead class="bg-elevated text-left text-toned">
              <tr>
                <th class="px-3 py-2">#</th>
                <th class="px-3 py-2">{{ t('table.player') }}</th>
                <th class="px-3 py-2 text-right whitespace-nowrap">EX SCORE</th>
                <th class="px-3 py-2 text-center">RANK</th>
                <th class="px-3 py-2">{{ t('table.clear') }}</th>
                <th v-if="lnPolicies.length" class="px-3 py-2">LN</th>
                <th class="px-3 py-2">RULE</th>
                <th class="px-3 py-2">OPTION</th>
                <th class="px-3 py-2 text-right whitespace-nowrap">MAX COMBO</th>
                <th class="px-2 py-2 text-right">BP</th>
                <th class="px-2 py-2 text-right">CB</th>
                <th class="px-3 py-2">{{ t('table.date') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="entry in ranking.ranking.entries"
                :key="entry.score.course_score_id"
                class="border-t border-default"
              >
                <td class="px-3 py-2 text-toned">{{ entry.rank }}</td>
                <td class="px-3 py-2">
                  <NuxtLink :to="localePath(`/players/${entry.player.id}`)" class="hover:underline">
                    {{ entry.player.display_name }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2 text-right">
                  <RankingExScore :score="entry.score.ex_score" :max="entry.score.max_ex_score" />
                </td>
                <td class="px-3 py-2 text-center">
                  <RankingScoreRank :score="entry.score.ex_score" :max="entry.score.max_ex_score" />
                </td>
                <td class="px-3 py-2">
                  <IrClearBadge :clear="entry.score.clear" />
                </td>
                <td v-if="lnPolicies.length" class="px-3 py-2 text-muted">
                  {{ lnPolicyLabel(entry.score.ln_policy) }}
                </td>
                <td class="px-3 py-2 text-muted">{{ ruleModeLabel(entry.score.rule_mode) }}</td>
                <td class="px-3 py-2 text-xs text-muted">{{ scoreOption(entry.score) }}</td>
                <td class="px-3 py-2 text-right">
                  <RankingMaxCombo
                    :combo="entry.score.max_combo"
                    :total-notes="totalNotesFromMaxExScore(entry.score.max_ex_score)"
                  />
                </td>
                <td class="px-2 py-2 text-right tabular-nums">{{ entry.score.bp }}</td>
                <td class="px-2 py-2 text-right tabular-nums">{{ entry.score.cb ?? '-' }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ formatScoreDate(entry.score.played_at) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <ScoreHistoryModal
          v-model:open="historyOpen"
          v-model:page="historyPage"
          :history="selfHistory"
          :pending="selfHistoryPending"
          :error-description="historyErrorDescription"
          :limit="historyLimit"
          :score-key="(score) => score.course_score_id"
          :score-bp="(score) => score.bp"
          :score-cb="(score) => score.cb"
          :show-ln="Boolean(lnPolicies.length)"
          :score-option="scoreOption"
        />
      </template>
    </section>
  </main>
</template>
