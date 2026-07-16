<script setup lang="ts">
import type {
  IrRanking,
  IrRankingEntry,
  IrRuleMode,
  IrScoreHistoryResult,
  LnScorePolicy,
} from '~~/bmz-ir-web/shared/types/ir'

interface ChartDetail {
  chart: {
    sha256: string
    md5: string
    title: string
    subtitle: string | null
    genre: string | null
    artist: string | null
    subartists: string[]
    mode: string
    level: number | null
    total: number | null
    judge_rank: number | null
    min_bpm: number | null
    max_bpm: number | null
    notes: number
    ln_notes: number
    cn_notes: number
    hcn_notes: number
    mine_notes: number
    has_random: boolean
    has_stop: boolean
    has_undefined_ln: boolean
    has_defined_ln: boolean
    has_defined_cn: boolean
    has_defined_hcn: boolean
    has_ln: boolean
    has_cn: boolean
    has_hcn: boolean
    has_mine: boolean
    source_url: string | null
    append_url: string | null
    headers: Record<string, string>
    created_at: number
    updated_at: number
  }
  stats: {
    global: { play_count: number; clear_count: number }
    self: { play_count: number; clear_count: number } | null
  }
}

const route = useRoute()
const localePath = useLocalePath()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const chartParam = computed(() =>
  String(route.params.sha256 ?? '')
    .trim()
    .toLowerCase(),
)
const paramError = ref<string | null>(null)

if (/^[0-9a-f]{32}$/.test(chartParam.value)) {
  try {
    const lookup = await $fetch<{ sha256: string }>('/api/v1/charts/lookup', {
      query: { md5: chartParam.value },
    })
    await navigateTo(localePath(`/charts/${lookup.sha256}`), { redirectCode: 301, replace: true })
  } catch {
    paramError.value = t('chart.md5NotFound')
  }
} else if (chartParam.value && !/^[0-9a-f]{64}$/.test(chartParam.value)) {
  paramError.value = t('chart.invalidId')
}

const sha256 = computed(() => (/^[0-9a-f]{64}$/.test(chartParam.value) ? chartParam.value : ''))
const canLoadChart = computed(() => sha256.value.length === 64)

type LnPolicyFilter = 'ALL' | LnScorePolicy
type RuleModeFilter = 'ALL' | IrRuleMode

const lnPolicy = ref<LnPolicyFilter>('ALL')
const lnPolicies: LnPolicyFilter[] = [
  'ALL',
  'AutoLn',
  'AutoCn',
  'AutoHcn',
  'ForceLn',
  'ForceCn',
  'ForceHcn',
]
const ruleMode = ref<RuleModeFilter>('ALL')
const ruleModes: RuleModeFilter[] = ['ALL', 'Beatoraja', 'Lr2Oraja', 'Dx']

const { data: detail, error: detailError } = await useFetch<ChartDetail>(
  () => `/api/v1/charts/${sha256.value}`,
  { immediate: canLoadChart.value, watch: [sha256] },
)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<IrRanking>(() => `/api/v1/charts/${sha256.value}/ranking`, {
  immediate: canLoadChart.value,
  watch: [sha256, lnPolicy, ruleMode],
  query: computed(() => ({
    scope: 'global',
    ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
    ...(ruleMode.value === 'ALL' ? {} : { rule_mode: ruleMode.value }),
  })),
})

const selfBestEntry = computed<IrRankingEntry | null>(
  () =>
    ranking.value?.ranking.self?.entry ??
    ranking.value?.ranking.entries.find((entry) => entry.relation.is_self) ??
    null,
)
const historyOpen = ref(false)
const historyPage = ref(1)
const historyLimit = 50
const historyOffset = computed(() => (historyPage.value - 1) * historyLimit)
const historyQuery = computed(() => ({
  scope: 'self',
  limit: historyLimit,
  offset: historyOffset.value,
  ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
  ...(ruleMode.value === 'ALL' ? {} : { rule_mode: ruleMode.value }),
}))
const {
  data: selfHistory,
  pending: selfHistoryPending,
  error: selfHistoryError,
  refresh: refreshSelfHistory,
} = await useFetch<IrScoreHistoryResult>(() => `/api/v1/charts/${sha256.value}/self-scores`, {
  immediate: false,
  watch: false,
  query: historyQuery,
})

const canShowSelfArea = computed(() => Boolean(selfBestEntry.value || detail.value?.stats.self))

async function openHistory() {
  historyOpen.value = true
  await refreshSelfHistory()
}

watch([lnPolicy, ruleMode, sha256], () => {
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

const copyMd5 = async () => {
  if (detail.value) {
    await navigator.clipboard.writeText(detail.value.chart.md5)
  }
}

const copySha256 = async () => {
  if (detail.value) {
    await navigator.clipboard.writeText(detail.value.chart.sha256)
  }
}

function formatScoreDate(value: string | null) {
  return value ? formatDateTime(value) : '-'
}

function formatArrange(value: string | null | undefined) {
  return value ? value.toUpperCase() : '-'
}

function formatArrangePair(score: { arrange_1p?: string | null; arrange_2p?: string | null }) {
  return `ARRANGE 1P ${formatArrange(score.arrange_1p)} / 2P ${formatArrange(score.arrange_2p)}`
}

const detailErrorDescription = computed(() =>
  detailError.value ? translateApiError(detailError.value, 'errors.chartLoadFailed') : '',
)
const rankingErrorDescription = computed(() =>
  rankingError.value ? translateApiError(rankingError.value, 'errors.rankingLoadFailed') : '',
)
const historyErrorDescription = computed(() =>
  selfHistoryError.value
    ? translateApiError(selfHistoryError.value, 'errors.historyLoadFailed')
    : '',
)
useSeoMeta({ title: () => detail.value?.chart.title ?? t('chart.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="paramError" color="error" :description="paramError" class="mb-6" />
      <UAlert
        v-else-if="detailError"
        color="error"
        :description="detailErrorDescription"
        class="mb-6"
      />
      <template v-else-if="detail">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="localePath('/charts')" class="hover:underline">{{
              t('charts.title')
            }}</NuxtLink>
          </p>
          <p>{{ detail.chart.genre }}</p>
          <h1 class="text-3xl font-semibold">
            {{ detail.chart.title }}
            <span v-if="detail.chart.subtitle" class="text-xl text-neutral-400">
              {{ detail.chart.subtitle }}
            </span>
          </h1>
          <p class="mt-2 text-sm text-neutral-300">
            {{ detail.chart.artist ?? '' }}
            <span v-if="detail.chart.subartists.length">
              {{ ` / ${detail.chart.subartists.join(' / ')}` }}
            </span>
          </p>
          <p class="mt-1 text-sm text-neutral-400">
            {{ detail.chart.mode }}
            <span v-if="detail.chart.level != null"> ☆{{ detail.chart.level }}</span>
            ・ {{ t('charts.notes', { count: detail.chart.notes }) }} ・
            {{
              t('chart.playClearCount', {
                plays: detail.stats.global.play_count,
                clears: detail.stats.global.clear_count,
              })
            }}
          </p>
          <p class="mt-3 text-sm text-neutral-400">
            md5 {{ detail.chart.md5 }}
            <UButton size="xs" variant="subtle" color="neutral" @click="copyMd5">{{
              t('chart.copy')
            }}</UButton>
          </p>
          <p class="mt-1 text-sm text-neutral-400">
            sha256 {{ detail.chart.sha256 }}
            <UButton size="xs" variant="subtle" color="neutral" @click="copySha256">{{
              t('chart.copy')
            }}</UButton>
          </p>
        </div>

        <div class="mb-4 flex flex-wrap items-center gap-3">
          LN <USelect v-model="lnPolicy" :items="lnPolicies" class="w-40" /> RULE
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
              <p class="text-xl font-semibold">
                EX
                <NuxtLink
                  :to="localePath(`/scores/${selfBestEntry.score.score_id}`)"
                  class="hover:underline"
                >
                  {{ selfBestEntry.score.ex_score }}
                </NuxtLink>
              </p>
              <p class="text-sm">
                CLEAR
                <NuxtLink
                  :to="
                    localePath(
                      `/scores/${selfBestEntry.score.source_score_ids?.clear ?? selfBestEntry.score.score_id}`,
                    )
                  "
                  class="hover:underline"
                >
                  {{ selfBestEntry.score.clear }}
                </NuxtLink>
              </p>
              <p class="text-sm">COMBO {{ selfBestEntry.score.max_combo }}</p>
              <p class="text-sm">BP {{ selfBestEntry.score.min_bp }}</p>
            </div>
            <p class="mt-1 text-xs text-neutral-500">
              {{ selfBestEntry.score.gauge }} / {{ selfBestEntry.score.ln_policy }} /
              {{ selfBestEntry.score.rule_mode }}
            </p>
            <p class="mt-1 text-xs text-neutral-500">
              {{ formatArrangePair(selfBestEntry.score) }}
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
                <th class="px-3 py-2">{{ t('table.conditions') }}</th>
                <th class="px-3 py-2 text-right">COMBO</th>
                <th class="px-3 py-2 text-right">BP</th>
                <th class="px-3 py-2">{{ t('table.input') }}</th>
                <th class="px-3 py-2">{{ t('table.date') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="entry in ranking.ranking.entries"
                :key="entry.score.score_id"
                class="border-t border-neutral-800"
              >
                <td class="px-3 py-2 text-neutral-300">{{ entry.rank }}</td>
                <td class="px-3 py-2">
                  <NuxtLink :to="localePath(`/players/${entry.player.id}`)" class="hover:underline">
                    {{ entry.player.display_name }}
                  </NuxtLink>
                  <UBadge v-if="entry.relation.is_rival" size="sm" color="warning" variant="subtle">
                    rival
                  </UBadge>
                </td>
                <td class="px-3 py-2 text-right font-medium">
                  <NuxtLink
                    :to="localePath(`/scores/${entry.score.score_id}`)"
                    class="hover:underline"
                  >
                    {{ entry.score.ex_score }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2">
                  <NuxtLink
                    :to="
                      localePath(
                        `/scores/${entry.score.source_score_ids?.clear ?? entry.score.score_id}`,
                      )
                    "
                    class="hover:underline"
                  >
                    {{ entry.score.clear }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2 text-neutral-400">
                  <p>
                    {{ entry.score.gauge }} / {{ entry.score.ln_policy }} /
                    {{ entry.score.rule_mode }}
                  </p>
                  <p class="text-xs text-neutral-500">
                    {{ formatArrangePair(entry.score) }}
                  </p>
                </td>
                <td class="px-3 py-2 text-right">{{ entry.score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.min_bp }}</td>
                <td class="px-3 py-2 text-neutral-400">{{ entry.score.device_type }}</td>
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
                    :key="score.score_id"
                    class="border-t border-neutral-800"
                  >
                    <td class="px-3 py-2 text-neutral-400">
                      {{ formatScoreDate(score.played_at ?? score.server_received_at) }}
                    </td>
                    <td class="px-3 py-2 text-right font-medium">
                      <NuxtLink
                        :to="localePath(`/scores/${score.score_id}`)"
                        class="hover:underline"
                      >
                        {{ score.ex_score }}
                      </NuxtLink>
                    </td>
                    <td class="px-3 py-2">{{ score.clear }}</td>
                    <td class="px-3 py-2 text-right">{{ score.max_combo }}</td>
                    <td class="px-3 py-2 text-right">{{ score.min_bp }}</td>
                    <td class="px-3 py-2 text-neutral-400">
                      <p>{{ score.gauge }} / {{ score.ln_policy }} / {{ score.rule_mode }}</p>
                      <p class="text-xs text-neutral-500">
                        {{ formatArrangePair(score) }}
                      </p>
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
