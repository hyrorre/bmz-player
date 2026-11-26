<script setup lang="ts">
import { IR_PLAYER_SCORE_PAGE_SIZE } from '~~/bmz-ir-web/shared/constants/ir'
import type { IrArrangeOptions, IrDifficultyLabel, IrRanking } from '~~/bmz-ir-web/shared/types/ir'
import type { RankingLnFilters } from '../../utils/rankingDisplay'

interface PlayerDetail {
  player: {
    id: string
    display_name: string
    bio: string | null
  }
  relationship: {
    is_self: boolean
    is_rival: boolean
    is_rivaled_by: boolean
  }
  ln_filters: RankingLnFilters
  pagination: IrRanking['ranking']['pagination']
  best_scores: (IrArrangeOptions & {
    id: string
    score_id: string
    best_ex_score_id: string | null
    best_clear_score_id: string | null
    chart_sha256: string
    ex_score: number
    max_ex_score: number | null
    clear_type: string
    clear_rank: number
    max_combo: number
    max_combo_notes: number | null
    min_bp: number
    min_cb: number
    device_type: string
    ln_policy: string
    rule_mode: string
    played_at: string | null
    server_received_at: string
    chart: {
      title: string
      subtitle: string | null
      artist: string | null
      mode: string
      has_ln: boolean
      difficulty_labels: IrDifficultyLabel[]
    } | null
  })[]
}

const route = useRoute()
const playerId = computed(() => String(route.params.id ?? ''))
const localePath = useLocalePath()
const listLocation = computed(() =>
  localePath({ path: '/players', query: listReturnQuery(route.query) }),
)
const { user } = useUserSession()
const { t } = useI18n()
const { formatNumber } = useLocaleFormat()
const { translateApiError } = useApiError()
const toast = useToast()
const rivalUpdating = ref(false)
const { expandedScoreRow, scoreDetailsId, toggleScoreDetails } = useScoreDisclosure()
const wideScoreTable = useWideScoreTable()

const selectableLnPolicies = availableLnFilterItems({
  has_ln: true,
  has_mixed_ln: true,
  has_auto_ln: true,
})
const lnPolicies = computed(() => availableLnFilterItems(data.value?.ln_filters))
const { lnPolicy, ruleMode, page, pageSize, maxPage, offset, pageLocation, correctOutOfRangePage } =
  useRankingQuery(lnPolicies, { queryKey: 'score_page', pageSize: IR_PLAYER_SCORE_PAGE_SIZE })
// A direct link can set an LN filter before the profile's available options arrive.
const scoreQuery = computed(() => ({
  limit: pageSize,
  offset: offset.value,
  ln_policy: selectableLnPolicies.find(
    ({ value }) => value !== 'ALL' && value === route.query.ln_policy,
  )?.value,
  rule_mode: ruleMode.value === 'ALL' ? undefined : ruleMode.value,
}))
const { data, pending, error } = await useFetch<PlayerDetail>(
  () => `/api/v1/players/${playerId.value}`,
  { key: () => `player-detail:${playerId.value}`, query: scoreQuery },
)
await correctOutOfRangePage(() => data.value?.pagination)
const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.playerLoadFailed') : '',
)

const canManageRival = computed(() =>
  Boolean(user.value && data.value && !data.value.relationship.is_self),
)

async function toggleRival() {
  if (!data.value || !canManageRival.value) return
  rivalUpdating.value = true
  const target = data.value.player.id
  const adding = !data.value.relationship.is_rival
  try {
    await $fetch('/api/v1/rivals', {
      method: 'POST',
      body: { target_player_id: target, action: adding ? 'add' : 'remove' },
    })
    const current = data.value
    if (current?.player.id === target) {
      data.value = { ...current, relationship: { ...current.relationship, is_rival: adding } }
    }
    toast.add({
      title: adding ? t('rivals.added') : t('rivals.removed'),
      color: 'success',
      icon: 'i-lucide-circle-check',
    })
  } catch (requestError) {
    toast.add({
      title: t('rivals.updateFailed'),
      description: translateApiError(requestError, 'errors.rivalUpdateFailed'),
      color: 'error',
      icon: 'i-lucide-circle-alert',
    })
  } finally {
    rivalUpdating.value = false
  }
}

useSeoMeta({ title: () => data.value?.player.display_name ?? t('players.player') })
</script>

<template>
  <main class="min-w-0">
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
      <UAlert v-if="error" color="error" :description="errorDescription" />
      <p v-else-if="pending && data?.player.id !== playerId" class="text-sm text-muted">
        {{ t('common.loading') }}
      </p>
      <template v-else-if="data">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="listLocation" class="hover:underline">{{ t('players.title') }}</NuxtLink>
          </p>
          <h1 class="text-3xl font-semibold">{{ data.player.display_name }}</h1>
          <p v-if="data.player.bio" class="mt-2 whitespace-pre-line text-sm text-neutral-300">
            {{ data.player.bio }}
          </p>
          <div class="mt-4 flex flex-wrap gap-2">
            <UButton
              color="neutral"
              icon="i-lucide-calendar-days"
              :to="{ path: localePath('/daily'), query: { player: data.player.id, mode: 'all' } }"
              variant="subtle"
            >
              {{ t('nav.daily') }}
            </UButton>
            <UButton
              color="neutral"
              icon="i-lucide-chart-bar-stacked"
              :to="localePath({ path: '/analysis', query: { player: data.player.id } })"
              variant="subtle"
            >
              {{ t('analysis.nav') }}
            </UButton>
            <UButton
              v-if="canManageRival"
              :color="data.relationship.is_rival ? 'error' : 'primary'"
              :icon="data.relationship.is_rival ? 'i-lucide-user-minus' : 'i-lucide-user-plus'"
              :loading="rivalUpdating"
              variant="subtle"
              @click="toggleRival"
            >
              {{ data.relationship.is_rival ? t('rivals.remove') : t('rivals.add') }}
            </UButton>
            <UButton
              v-if="data.relationship.is_rival"
              color="warning"
              icon="i-lucide-git-compare-arrows"
              :to="{ path: localePath('/rivals'), query: { player: data.player.id } }"
              variant="subtle"
            >
              {{ t('rivals.compare') }}
            </UButton>
            <UBadge
              v-if="data.relationship.is_rivaled_by"
              color="warning"
              icon="i-lucide-eye"
              variant="subtle"
            >
              {{ t('rivals.rivaledBy') }}
            </UBadge>
          </div>
        </div>

        <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
          <h2 class="text-lg font-medium">{{ t('players.bestScores') }}</h2>
          <div class="flex flex-wrap items-center gap-3">
            <UFormField v-if="lnPolicies.length" label="LN" orientation="horizontal">
              <USelect v-model="lnPolicy" :items="lnPolicies" class="w-28" />
            </UFormField>
            <UFormField label="RULE" orientation="horizontal">
              <USelect v-model="ruleMode" :items="ruleModeItems" class="w-32" />
            </UFormField>
          </div>
        </div>
        <RankingPagination
          v-if="!pending"
          :page="page"
          :page-size="pageSize"
          :max-page="maxPage"
          :total="data.pagination.total"
          :total-label="t('players.totalScores', { total: formatNumber(data.pagination.total) })"
          :page-location="pageLocation"
        />
        <p v-if="pending" class="text-sm text-muted">{{ t('common.loading') }}</p>
        <p v-else-if="!data.best_scores.length" class="text-sm text-muted">
          {{
            t(
              lnPolicy === 'ALL' && ruleMode === 'ALL' && page === 1
                ? 'players.noScoresYet'
                : 'ranking.noScores',
            )
          }}
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-default">
          <table class="w-full text-sm">
            <thead class="bg-elevated text-left text-toned">
              <tr>
                <th class="px-3 py-2">{{ t('table.chart') }}</th>
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
              <template v-for="score in data.best_scores" :key="score.id">
                <tr class="border-t border-default">
                  <td class="min-w-32 px-3 py-2 wrap-anywhere sm:min-w-48">
                    <NuxtLink
                      :to="localePath(`/charts/${score.chart_sha256}`)"
                      class="font-medium hover:underline"
                    >
                      {{ score.chart?.title ?? score.chart_sha256.slice(0, 12) }}
                      <span v-if="score.chart?.subtitle" class="text-muted">{{
                        score.chart.subtitle
                      }}</span>
                    </NuxtLink>
                    <p v-if="score.chart?.artist" class="text-xs text-muted">
                      {{ score.chart.artist }}
                    </p>
                    <DifficultyTableBadges
                      v-if="score.chart?.difficulty_labels.length"
                      :labels="score.chart.difficulty_labels"
                      class="mt-1"
                    />
                  </td>
                  <td class="px-3 py-2 text-right">
                    <RankingExScore :score="score.ex_score" :max="score.max_ex_score" />
                  </td>
                  <td class="px-3 py-2 text-center">
                    <RankingScoreRank :score="score.ex_score" :max="score.max_ex_score" />
                  </td>
                  <td class="px-3 py-2">
                    <IrClearBadge :clear="score.clear_type" :rank="score.clear_rank" compact />
                  </td>
                  <td v-if="lnPolicies.length" class="px-3 py-2 text-muted">
                    {{ score.chart?.has_ln ? lnPolicyLabel(score.ln_policy) : '-' }}
                  </td>
                  <td class="px-3 py-2 text-muted">{{ ruleModeLabel(score.rule_mode) }}</td>
                  <td class="px-3 py-2 text-xs whitespace-nowrap text-muted">
                    {{ formatRankingArrange(score, score.chart?.mode ?? null) }}
                  </td>
                  <td class="px-3 py-2 text-right">
                    <RankingMaxCombo
                      :combo="score.max_combo"
                      :total-notes="score.max_combo_notes"
                    />
                  </td>
                  <td class="px-2 py-2 text-right tabular-nums">{{ score.min_bp }}</td>
                  <td class="px-2 py-2 text-right tabular-nums">{{ score.min_cb }}</td>
                  <td class="hidden px-3 py-2 text-muted xl:table-cell">{{ score.device_type }}</td>
                  <td class="hidden px-3 py-2 text-muted xl:table-cell">
                    <ScoreTableDateTime :value="score.played_at ?? score.server_received_at" />
                  </td>
                  <td class="sticky right-0 z-10 bg-default px-1 py-1">
                    <ScoreDetailToggle
                      :expanded="expandedScoreRow === score.id"
                      :controls="scoreDetailsId"
                      @toggle="toggleScoreDetails(score.id)"
                    />
                  </td>
                </tr>
                <tr
                  v-if="expandedScoreRow === score.id"
                  class="border-t border-default bg-elevated"
                >
                  <td :colspan="10 + (lnPolicies.length ? 1 : 0) + (wideScoreTable ? 2 : 0)">
                    <ScoreInlineDetails
                      :id="scoreDetailsId"
                      :score-id="score.best_ex_score_id ?? score.score_id"
                      :other-score-id="score.best_clear_score_id"
                    />
                  </td>
                </tr>
              </template>
            </tbody>
          </table>
        </div>
      </template>
    </section>
  </main>
</template>
