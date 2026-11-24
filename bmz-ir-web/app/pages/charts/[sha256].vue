<script setup lang="ts">
import type {
  IrChartLnProfile,
  IrDifficultyLabel,
  IrRanking,
  IrRankingEntry,
  IrRankingScope,
  IrScoreHistoryResult,
} from '~~/bmz-ir-web/shared/types/ir'
import { readDifficultyFilter } from '~~/bmz-ir-web/shared/utils/difficulty_filter'

interface ChartDetail {
  chart: IrChartLnProfile & {
    sha256: string
    md5: string | null
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
    hln_notes: number
    mine_notes: number
    has_random: boolean
    has_stop: boolean
    has_ln: boolean
    has_cn: boolean
    has_hcn: boolean
    has_hln: boolean
    difficulty_labels: IrDifficultyLabel[]
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
const listLocation = computed(() =>
  localePath({
    path: '/charts',
    query: { ...listReturnQuery(route.query), ...readDifficultyFilter(route.query) },
  }),
)
const { user } = useUserSession()
const { t } = useI18n()
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
    await navigateTo(localePath({ path: `/charts/${lookup.sha256}`, query: route.query }), {
      redirectCode: 301,
      replace: true,
    })
  } catch {
    paramError.value = t('chart.md5NotFound')
  }
} else if (chartParam.value && !/^[0-9a-f]{64}$/.test(chartParam.value)) {
  paramError.value = t('chart.invalidId')
}

const sha256 = computed(() => (/^[0-9a-f]{64}$/.test(chartParam.value) ? chartParam.value : ''))
const canLoadChart = computed(() => sha256.value.length === 64)

type RankingScopeFilter = Extract<IrRankingScope, 'global' | 'self_and_rivals'>
const rankingScopes = computed(() => [
  { label: t('ranking.global'), value: 'global' as const },
  ...(user.value ? [{ label: t('ranking.selfAndRivals'), value: 'self_and_rivals' as const }] : []),
])

const { data: detail, error: detailError } = await useFetch<ChartDetail>(
  () => `/api/v1/charts/${sha256.value}`,
  { immediate: canLoadChart.value, watch: [sha256] },
)
const chartHashes = computed(() => {
  if (!detail.value) return []
  const { md5, sha256 } = detail.value.chart
  return [...(md5 ? [{ label: 'MD5', value: md5 }] : []), { label: 'SHA256', value: sha256 }]
})
const externalLinks = computed(() => {
  if (!detail.value) return []
  const { md5, sha256 } = detail.value.chart
  const links = [
    { label: 'rianIR', to: `https://rianir.link/ranking?sha256=${sha256}` },
    { label: 'Mocha', to: `https://mocha-repository.info/song.php?sha256=${sha256}` },
    { label: 'MinIR', to: `https://www.gaftalk.com/minir/#/viewer/song/${sha256}/0` },
    { label: 'Kaleid IR', to: `https://kaleidir.com/charts/${sha256}` },
    { label: 'Bokutachi IR', to: `https://boku.tachi.ac/search?q=${sha256}&hasPlayedGame=false` },
  ]
  if (md5 && /^[0-9a-f]{32}$/i.test(md5)) {
    links.push(
      { label: 'BMS-IR', to: `https://bms-ir.org/new/song?songmd5=${md5}` },
      { label: 'Stellaverse IR', to: `https://ir.stellabms.xyz/redirecthash/${md5}` },
      { label: 'BMS Score Viewer', to: `https://bms-score-viewer.pages.dev/view?md5=${md5}` },
    )
  }
  return links
})
const lnPolicies = computed(() => lnFilterItems(detail.value?.chart))
const {
  lnPolicy,
  ruleMode,
  ruleQuery,
  rankingQuery,
  setRankingFilter,
  page,
  pageSize,
  maxPage,
  pageLocation,
} = useRankingQuery(lnPolicies)
const rankingScope = computed<RankingScopeFilter>({
  get: () => (user.value && route.query.scope === 'self_and_rivals' ? 'self_and_rivals' : 'global'),
  set(value) {
    if (value !== rankingScope.value) {
      void setRankingFilter('scope', value === 'global' ? undefined : value)
    }
  },
})
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<IrRanking>(() => `/api/v1/charts/${sha256.value}/ranking`, {
  immediate: canLoadChart.value,
  query: computed(() => ({ ...rankingQuery.value, scope: rankingScope.value })),
})

const selfBestEntry = computed<IrRankingEntry | null>(
  () =>
    ranking.value?.ranking.self?.entry ??
    ranking.value?.ranking.entries.find((entry) => entry.relation.is_self) ??
    null,
)
const {
  open: historyOpen,
  page: historyPage,
  limit: historyLimit,
  data: selfHistory,
  pending: selfHistoryPending,
  error: selfHistoryError,
  openHistory,
} = useSelfScoreHistory<IrScoreHistoryResult>(
  () => `/api/v1/charts/${sha256.value}/self-scores`,
  () => ({ ...ruleQuery.value, scope: 'self' }),
)

const canShowSelfArea = computed(() => Boolean(selfBestEntry.value || detail.value?.stats.self))
const { expandedScoreRow, scoreDetailsId, toggleScoreDetails } = useScoreDisclosure()
const wideScoreTable = useWideScoreTable()

const copiedHash = shallowRef<{ hash: string }>()
watch(copiedHash, (copied, _, onCleanup) => {
  if (!copied) return
  const timeout = window.setTimeout(() => {
    copiedHash.value = undefined
  }, 1500)
  onCleanup(() => window.clearTimeout(timeout))
})

async function copyHash(hash: string) {
  try {
    await navigator.clipboard.writeText(hash)
  } catch {
    return
  }
  copiedHash.value = { hash }
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
  <main class="min-w-0">
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
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
            <NuxtLink :to="listLocation" class="hover:underline">{{ t('charts.title') }}</NuxtLink>
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
          <DifficultyTableBadges :labels="detail.chart.difficulty_labels" class="mt-2" />
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <UButton
              v-for="hash in chartHashes"
              :key="hash.label"
              type="button"
              :title="t('chart.copy')"
              :trailing-icon="copiedHash?.hash === hash.value ? 'i-lucide-check' : 'i-lucide-copy'"
              :ui="{
                trailingIcon: copiedHash?.hash === hash.value ? 'text-success' : 'text-muted',
              }"
              color="neutral"
              variant="subtle"
              size="sm"
              class="max-w-full min-w-0 cursor-pointer rounded-full text-start"
              @click="copyHash(hash.value)"
            >
              <span class="shrink-0 font-semibold">{{ hash.label }}</span>
              <span class="min-w-0 font-mono text-xs break-all text-toned">{{ hash.value }}</span>
            </UButton>
            <span class="sr-only" role="status">{{ copiedHash ? t('chart.copied') : '' }}</span>
          </div>
          <nav
            aria-labelledby="chart-external-services"
            class="mt-4 flex flex-wrap items-center gap-2"
          >
            <span id="chart-external-services" class="text-sm font-medium text-muted">
              {{ t('chart.externalServices') }}
            </span>
            <UButton
              v-for="link in externalLinks"
              :key="link.label"
              :to="link.to"
              :label="link.label"
              target="_blank"
              rel="noopener noreferrer"
              trailing-icon="i-lucide-external-link"
              :ui="{ trailingIcon: 'text-muted' }"
              color="neutral"
              variant="subtle"
              size="sm"
            />
          </nav>
        </div>

        <div class="mb-4 flex flex-wrap items-center gap-3">
          <UFormField v-if="user" :label="t('ranking.scope')" orientation="horizontal">
            <USelect v-model="rankingScope" :items="rankingScopes" class="w-48" />
          </UFormField>
          <UFormField v-if="lnPolicies.length" label="LN" orientation="horizontal">
            <USelect v-model="lnPolicy" :items="lnPolicies" class="w-28" />
          </UFormField>
          <UFormField label="RULE" orientation="horizontal">
            <USelect v-model="ruleMode" :items="ruleModeItems" class="w-32" />
          </UFormField>
        </div>

        <div v-if="canShowSelfArea" class="mb-4 rounded-lg border border-default p-4">
          <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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
                    :total-notes="selfBestEntry.score.max_combo_notes"
                  />
                </p>
                <p class="text-sm">BP {{ selfBestEntry.score.min_bp }}</p>
                <p class="text-sm">CB {{ selfBestEntry.score.min_cb }}</p>
              </div>
              <p class="mt-1 text-xs text-neutral-500">
                {{ selfBestEntry.score.gauge }} ·
                <template v-if="lnPolicies.length"
                  >{{ lnPolicyLabel(selfBestEntry.score.ln_policy) }} ·</template
                >
                {{ ruleModeLabel(selfBestEntry.score.rule_mode) }} ·
                {{ formatRankingArrange(selfBestEntry.score, detail.chart.mode) }}
              </p>
            </div>
            <p v-else class="text-sm text-neutral-400">{{ t('ranking.noPersonalBest') }}</p>
            <div class="flex shrink-0 items-center justify-end gap-2">
              <UButton icon="i-lucide-list" color="neutral" variant="subtle" @click="openHistory">
                {{ t('ranking.selfHistory') }}
              </UButton>
              <ScoreDetailToggle
                v-if="selfBestEntry"
                :expanded="expandedScoreRow === 'personal-best'"
                :controls="scoreDetailsId"
                @toggle="toggleScoreDetails('personal-best')"
              />
            </div>
          </div>
          <ScoreInlineDetails
            v-if="selfBestEntry && expandedScoreRow === 'personal-best'"
            :id="scoreDetailsId"
            :score-id="
              selfBestEntry.score.source_score_ids?.ex_score ?? selfBestEntry.score.score_id
            "
            :other-score-id="selfBestEntry.score.source_score_ids?.clear"
            class="mt-3 border-t border-default"
          >
            <ScoreRowMetadata
              :device="selfBestEntry.score.device_type"
              :date="selfBestEntry.score.played_at"
            />
          </ScoreInlineDetails>
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
                <th class="hidden px-3 py-2 xl:table-cell">{{ t('table.input') }}</th>
                <th class="hidden w-px px-3 py-2 whitespace-nowrap xl:table-cell">
                  {{ t('table.date') }}
                </th>
                <th scope="col" class="sticky right-0 z-10 w-px bg-elevated px-1">
                  <span class="sr-only">{{ t('score.title') }}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              <template v-for="entry in ranking.ranking.entries" :key="entry.score.score_id">
                <tr class="border-t border-default">
                  <td class="px-3 py-2 text-toned">{{ entry.rank }}</td>
                  <td class="px-3 py-2">
                    <NuxtLink
                      :to="localePath(`/players/${entry.player.id}`)"
                      class="hover:underline"
                    >
                      {{ entry.player.display_name }}
                    </NuxtLink>
                    <UBadge
                      v-if="entry.relation.is_rival"
                      size="sm"
                      color="warning"
                      variant="subtle"
                    >
                      rival
                    </UBadge>
                  </td>
                  <td class="px-3 py-2 text-right">
                    <RankingExScore :score="entry.score.ex_score" :max="entry.score.max_ex_score" />
                  </td>
                  <td class="px-3 py-2 text-center">
                    <RankingScoreRank
                      :score="entry.score.ex_score"
                      :max="entry.score.max_ex_score"
                    />
                  </td>
                  <td class="px-3 py-2">
                    <IrClearBadge :clear="entry.score.clear" compact />
                  </td>
                  <td v-if="lnPolicies.length" class="px-3 py-2 text-muted">
                    {{ lnPolicyLabel(entry.score.ln_policy) }}
                  </td>
                  <td class="px-3 py-2 text-muted">{{ ruleModeLabel(entry.score.rule_mode) }}</td>
                  <td class="px-3 py-2 text-xs text-muted">
                    {{ formatRankingArrange(entry.score, detail.chart.mode) }}
                  </td>
                  <td class="px-3 py-2 text-right">
                    <RankingMaxCombo
                      :combo="entry.score.max_combo"
                      :total-notes="entry.score.max_combo_notes"
                    />
                  </td>
                  <td class="px-2 py-2 text-right tabular-nums">{{ entry.score.min_bp }}</td>
                  <td class="px-2 py-2 text-right tabular-nums">{{ entry.score.min_cb }}</td>
                  <td class="hidden px-3 py-2 text-muted xl:table-cell">
                    {{ entry.score.device_type }}
                  </td>
                  <td class="hidden px-3 py-2 text-muted xl:table-cell">
                    <ScoreTableDateTime :value="entry.score.played_at" />
                  </td>
                  <td class="sticky right-0 z-10 bg-default px-1 py-1">
                    <ScoreDetailToggle
                      :expanded="expandedScoreRow === entry.score.score_id"
                      :controls="scoreDetailsId"
                      @toggle="toggleScoreDetails(entry.score.score_id)"
                    />
                  </td>
                </tr>
                <tr
                  v-if="expandedScoreRow === entry.score.score_id"
                  class="border-t border-default bg-elevated"
                >
                  <td :colspan="11 + (lnPolicies.length ? 1 : 0) + (wideScoreTable ? 2 : 0)">
                    <ScoreInlineDetails
                      :id="scoreDetailsId"
                      :score-id="entry.score.source_score_ids?.ex_score ?? entry.score.score_id"
                      :other-score-id="entry.score.source_score_ids?.clear"
                    >
                      <ScoreRowMetadata
                        :device="entry.score.device_type"
                        :date="entry.score.played_at"
                        class="xl:hidden"
                      />
                    </ScoreInlineDetails>
                  </td>
                </tr>
              </template>
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
          :score-key="(score) => score.score_id"
          :score-bp="(score) => score.min_bp"
          :score-cb="(score) => score.min_cb"
          :score-detail-id="(score) => score.score_id"
          :show-ln="Boolean(lnPolicies.length)"
          :score-option="(score) => formatRankingArrange(score, detail?.chart.mode ?? null)"
        />
      </template>
    </section>
  </main>
</template>
