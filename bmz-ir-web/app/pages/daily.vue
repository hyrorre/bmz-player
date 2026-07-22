<script setup lang="ts">
import type {
  IrDailyBestSnapshot,
  IrDailyChartResult,
  IrDailyReport,
  IrDailyRuleResult,
} from '../../shared/types/ir'

interface DailyRow {
  chart: IrDailyChartResult['chart']
  difficultyLabels: IrDailyChartResult['difficulty_labels']
  ruleCount: number
  result: IrDailyRuleResult
}

const route = useRoute()
const localePath = useLocalePath()
const { t } = useI18n()
const { formatNumber: formatLocaleNumber, formatDate, compareText } = useLocaleFormat()
const { translateApiError } = useApiError()
const initialDate = ref(currentDailyDate(0))
const dateInput = ref(queryString(route.query.date) ?? initialDate.value)

const apiQuery = computed(() => {
  const query: Record<string, string> = {
    date: queryString(route.query.date) ?? initialDate.value,
    mode: 'all',
  }
  const player = queryString(route.query.player)
  if (player) query.player = player
  return query
})

const { data, status, error } = await useFetch<IrDailyReport>('/api/v1/daily', {
  query: apiQuery,
})

watch(
  () => data.value?.date,
  (date) => {
    if (date) dateInput.value = date
  },
  { immediate: true },
)

watch(
  data,
  async (report) => {
    if (!report || queryString(route.query.date)) return
    const today = currentDailyDate(report.boundary_minutes)
    await navigateDate(today, true)
  },
  { immediate: true },
)

const rows = computed<DailyRow[]>(() =>
  (data.value?.charts ?? [])
    .flatMap((chart) =>
      chart.rules.map((result) => ({
        chart: chart.chart,
        difficultyLabels: chart.difficulty_labels,
        ruleCount: chart.rules.length,
        result,
      })),
    )
    .sort((left, right) => {
      const leftLevel = left.difficultyLabels.map(difficultyLabel).join(',')
      const rightLevel = right.difficultyLabels.map(difficultyLabel).join(',')
      return compareText(leftLevel, rightLevel) || compareText(left.chart.title, right.chart.title)
    }),
)

const todayDate = computed(() => currentDailyDate(data.value?.boundary_minutes ?? 0))
const boundaryLabel = computed(() => formatBoundary(data.value?.boundary_minutes ?? 0))

function queryString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null
}

function currentDailyDate(boundaryMinutes: number): string {
  const shifted = Date.now() + (9 * 60 - boundaryMinutes) * 60_000
  return new Date(shifted).toISOString().slice(0, 10)
}

function shiftDate(date: string, days: number): string {
  const shifted = new Date(`${date}T00:00:00.000Z`)
  shifted.setUTCDate(shifted.getUTCDate() + days)
  return shifted.toISOString().slice(0, 10)
}

async function navigateDate(date: string, replace = false) {
  const query: Record<string, string> = { date, mode: 'all' }
  const player = queryString(route.query.player)
  if (player) query.player = player
  await navigateTo({ path: localePath('/daily'), query }, { replace })
}

function onDateChange() {
  if (dateInput.value) void navigateDate(dateInput.value)
}

function difficultyLabel(label: IrDailyChartResult['difficulty_labels'][number]): string {
  return `${label.symbol}${label.level}`
}

function formatNumber(value: number): string {
  return formatLocaleNumber(value)
}

function formatAccuracy(value: number | null): string {
  return value === null ? '-' : `${value.toFixed(2)}%`
}

function formatPlayTime(value: number): string {
  const totalSeconds = Math.floor(value / 1000)
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
}

function formatBoundary(value: number): string {
  const hours = Math.floor(value / 60)
  const minutes = value % 60
  return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')} JST`
}

function displayDate(value: string): string {
  return formatDate(new Date(`${value}T12:00:00+09:00`))
}

function clearLabel(value: string | undefined): string {
  const labels: Record<string, string> = {
    no_play: 'NO PLAY',
    NoPlay: 'NO PLAY',
    failed: 'FAILED',
    Failed: 'FAILED',
    assisted_easy_clear: 'ASSIST',
    AssistEasy: 'ASSIST',
    LightAssistEasy: 'ASSIST',
    easy_clear: 'EASY',
    Easy: 'EASY',
    clear: 'CLEAR',
    Normal: 'CLEAR',
    hard_clear: 'HARD',
    Hard: 'HARD',
    ex_hard_clear: 'EX HARD',
    ExHard: 'EX HARD',
    full_combo: 'FULL COMBO',
    FullCombo: 'FULL COMBO',
    perfect: 'PERFECT',
    Perfect: 'PERFECT',
    Max: 'MAX',
  }
  return value ? (labels[value] ?? value.toUpperCase()) : '-'
}

function clearColor(rank: number | undefined) {
  if ((rank ?? 0) >= 7) return 'warning' as const
  if ((rank ?? 0) >= 5) return 'error' as const
  if ((rank ?? 0) >= 3) return 'success' as const
  return 'neutral' as const
}

function scoreRate(result: IrDailyRuleResult): number | null {
  return scoreRateForSnapshot(result.after)
}

function scoreRateForSnapshot(snapshot: IrDailyBestSnapshot): number | null {
  const score = snapshot.ex_score
  const notes = snapshot.ex_score_notes
  return score !== null && notes !== null && notes > 0 ? (score / (notes * 2)) * 100 : null
}

function scoreGrade(rate: number | null): string {
  if (rate === null) return '-'
  if (rate >= 100) return 'MAX'
  if (rate >= 88.888_888) return 'AAA'
  if (rate >= 77.777_777) return 'AA'
  if (rate >= 66.666_666) return 'A'
  if (rate >= 55.555_555) return 'B'
  if (rate >= 44.444_444) return 'C'
  if (rate >= 33.333_333) return 'D'
  if (rate >= 22.222_222) return 'E'
  return 'F'
}

function signedDelta(value: number): string {
  return value > 0 ? `+${value}` : String(value)
}

function ruleLabel(result: IrDailyRuleResult): string {
  return `${result.rule.ln_policy} / ${result.rule.double_option} / ${result.rule.rule_mode}`
}

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.dailyLoadFailed') : '',
)
useSeoMeta({ title: () => t('daily.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
      <UAlert
        v-if="error"
        color="error"
        icon="i-lucide-circle-alert"
        :description="errorDescription"
      />
      <div v-else-if="status === 'pending'" class="py-16 text-center text-muted">
        <UIcon name="i-lucide-loader-circle" class="mx-auto mb-3 size-8 animate-spin" />
        {{ t('common.loading') }}
      </div>

      <template v-else-if="data">
        <section
          class="rounded-t-2xl border border-muted bg-elevated px-5 py-8 text-center shadow-lg sm:px-8"
        >
          <p class="mb-3 text-sm font-medium text-primary">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal sm:text-4xl">
            {{ data.player.display_name }}
          </h1>
          <p class="mt-3 text-base text-muted">
            {{ t('daily.reportForDate', { date: displayDate(data.date) }) }}
          </p>
          <div class="mt-4">
            <UBadge color="neutral" variant="subtle">ALL</UBadge>
          </div>

          <div class="mx-auto mt-7 grid max-w-4xl grid-cols-2 gap-3 sm:grid-cols-4">
            <div class="rounded-xl border border-muted bg-default/50 p-4">
              <p class="text-xs font-medium tracking-wider text-muted">PLAY NOTES</p>
              <p class="mt-1 text-2xl font-semibold">{{ formatNumber(data.summary.play_notes) }}</p>
            </div>
            <div class="rounded-xl border border-muted bg-default/50 p-4">
              <p class="text-xs font-medium tracking-wider text-muted">CLEAR / PLAY</p>
              <p class="mt-1 text-2xl font-semibold">
                {{ data.summary.clear_count }} / {{ data.summary.play_count }}
              </p>
            </div>
            <div class="rounded-xl border border-muted bg-default/50 p-4">
              <p class="text-xs font-medium tracking-wider text-muted">ACCURACY</p>
              <p class="mt-1 text-2xl font-semibold">
                {{ formatAccuracy(data.summary.accuracy) }}
              </p>
            </div>
            <div class="rounded-xl border border-muted bg-default/50 p-4">
              <p class="text-xs font-medium tracking-wider text-muted">PLAY TIME</p>
              <p class="mt-1 text-2xl font-semibold">
                {{ formatPlayTime(data.summary.play_time_ms) }}
              </p>
              <p v-if="data.summary.play_time_unknown_count" class="mt-1 text-xs text-muted">
                {{ t('daily.unknownPlayTimes', { count: data.summary.play_time_unknown_count }) }}
              </p>
            </div>
          </div>
        </section>

        <section class="rounded-b-2xl border-x border-b border-muted bg-default p-4 sm:p-6">
          <div class="mb-6 flex flex-wrap items-center justify-center gap-2">
            <UButton
              color="neutral"
              icon="i-lucide-chevron-left"
              variant="subtle"
              @click="navigateDate(shiftDate(data.date, -1))"
            >
              {{ t('daily.previousDay') }}
            </UButton>
            <UInput
              v-model="dateInput"
              :aria-label="t('daily.resultDate')"
              :max="todayDate"
              type="date"
              @change="onDateChange"
            />
            <UButton color="neutral" variant="subtle" @click="navigateDate(todayDate)">
              {{ t('daily.today') }}
            </UButton>
            <UButton
              color="neutral"
              :disabled="data.date >= todayDate"
              trailing-icon="i-lucide-chevron-right"
              variant="subtle"
              @click="navigateDate(shiftDate(data.date, 1))"
            >
              {{ t('daily.nextDay') }}
            </UButton>
          </div>

          <p class="mb-4 text-center text-xs text-muted">
            {{
              t('daily.boundarySummary', {
                boundary: boundaryLabel,
                count: data.summary.chart_count,
              })
            }}
          </p>

          <div v-if="rows.length" class="overflow-x-auto rounded-xl border border-muted">
            <table class="w-full min-w-[760px] text-sm">
              <thead class="bg-elevated text-left text-muted">
                <tr>
                  <th class="w-32 px-4 py-3">LV</th>
                  <th class="px-4 py-3">TITLE</th>
                  <th class="w-52 px-4 py-3">CLEAR</th>
                  <th class="w-56 px-4 py-3">SCORE</th>
                  <th class="w-32 px-4 py-3">MISS</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="row in rows"
                  :key="`${row.chart.sha256}-${ruleLabel(row.result)}`"
                  class="border-t border-muted"
                >
                  <td class="px-4 py-3 align-top">
                    <div v-if="row.difficultyLabels.length" class="flex flex-wrap gap-1">
                      <UBadge
                        v-for="label in row.difficultyLabels"
                        :key="`${label.table_id}-${label.level}`"
                        color="primary"
                        size="sm"
                        variant="subtle"
                        :title="label.table_name"
                      >
                        {{ difficultyLabel(label) }}
                      </UBadge>
                    </div>
                    <UBadge v-else color="neutral" size="sm" variant="subtle">
                      ☆{{ row.chart.level ?? '?' }}
                    </UBadge>
                  </td>
                  <td class="max-w-100 px-4 py-3 align-top">
                    <NuxtLink
                      :to="localePath(`/charts/${row.chart.sha256}`)"
                      class="font-semibold text-highlighted hover:underline"
                    >
                      {{ row.chart.title || row.chart.sha256.slice(0, 12) }}
                    </NuxtLink>
                    <p v-if="row.chart.subtitle" class="mt-0.5 text-xs text-muted">
                      {{ row.chart.subtitle }}
                    </p>
                    <p v-if="row.ruleCount > 1" class="mt-1 text-xs text-muted">
                      {{ ruleLabel(row.result) }} ・ {{ row.result.plays }} PLAY
                    </p>
                  </td>
                  <td class="px-4 py-3 align-top">
                    <div class="flex flex-wrap items-center gap-1.5">
                      <template v-if="row.result.updated_fields.clear && row.result.before.clear">
                        <UBadge :color="clearColor(row.result.before.clear.rank)" variant="subtle">
                          {{ clearLabel(row.result.before.clear.type) }}
                        </UBadge>
                        <span class="text-muted">»</span>
                      </template>
                      <UBadge :color="clearColor(row.result.after.clear?.rank)" variant="subtle">
                        {{ clearLabel(row.result.after.clear?.type) }}
                      </UBadge>
                    </div>
                  </td>
                  <td class="px-4 py-3 align-top">
                    <div class="font-semibold">
                      <template
                        v-if="
                          row.result.updated_fields.ex_score && row.result.before.ex_score !== null
                        "
                      >
                        <UBadge color="neutral" size="sm" variant="subtle">
                          {{ scoreGrade(scoreRateForSnapshot(row.result.before)) }}
                        </UBadge>
                        <span class="text-muted">{{ row.result.before.ex_score }}</span>
                        <span class="mx-1.5 text-muted">»</span>
                      </template>
                      <UBadge color="warning" size="sm" variant="subtle">
                        {{ scoreGrade(scoreRate(row.result)) }}
                      </UBadge>
                      <span class="ml-1.5">{{ row.result.after.ex_score ?? '-' }}</span>
                    </div>
                    <div class="mt-1 text-xs text-muted">
                      <template
                        v-if="
                          row.result.updated_fields.ex_score && row.result.before.ex_score !== null
                        "
                      >
                        {{ formatAccuracy(scoreRateForSnapshot(row.result.before)) }}
                        <span class="mx-1">»</span>
                      </template>
                      {{ formatAccuracy(scoreRate(row.result)) }}
                      <span
                        v-if="
                          row.result.updated_fields.ex_score &&
                          row.result.before.ex_score !== null &&
                          row.result.after.ex_score !== null
                        "
                        class="ml-1 font-medium text-error"
                      >
                        {{ signedDelta(row.result.after.ex_score - row.result.before.ex_score) }}
                      </span>
                    </div>
                  </td>
                  <td class="px-4 py-3 align-top">
                    <template
                      v-if="row.result.updated_fields.min_bp && row.result.before.min_bp !== null"
                    >
                      <span class="text-muted">{{ row.result.before.min_bp }}</span>
                      <span class="mx-1.5 text-muted">»</span>
                    </template>
                    <span class="font-semibold">{{ row.result.after.min_bp ?? '-' }}</span>
                    <p
                      v-if="
                        row.result.updated_fields.min_bp &&
                        row.result.before.min_bp !== null &&
                        row.result.after.min_bp !== null
                      "
                      class="mt-1 text-xs font-medium text-success"
                    >
                      {{ signedDelta(row.result.after.min_bp - row.result.before.min_bp) }}
                    </p>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-else class="rounded-xl border border-dashed border-muted py-14 text-center">
            <UIcon name="i-lucide-calendar-x" class="mx-auto mb-3 size-8 text-muted" />
            <p class="font-medium">{{ t('daily.empty') }}</p>
          </div>
        </section>
      </template>
    </section>
  </main>
</template>
