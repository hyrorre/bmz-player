<script setup lang="ts">
import type {
  IrRivalComparison,
  IrRivalComparisonOutcome,
  IrRivalEntry,
  IrRivalsResponse,
} from '../../shared/types/ir'

const route = useRoute()
const localePath = useLocalePath()
const { user } = useUserSession()
const { t } = useI18n()
const { translateApiError } = useApiError()
const toast = useToast()
const { expandedScoreRow, scoreDetailsId, toggleScoreDetails } = useScoreDisclosure()

if (!user.value) {
  await navigateTo(localePath('/login'))
}

const {
  data: rivalLists,
  status: rivalStatus,
  error: rivalError,
  refresh: refreshRivals,
} = await useFetch<IrRivalsResponse>('/api/v1/rivals')

const selectedRivalId = computed({
  get() {
    const requestedPlayerId = queryString(route.query.player)
    return (
      rivalLists.value?.rivals.find((entry) => entry.player_id === requestedPlayerId)?.player_id ??
      rivalLists.value?.rivals[0]?.player_id ??
      ''
    )
  },
  set(playerId) {
    if (playerId !== selectedRivalId.value) {
      void navigateTo({
        path: route.path,
        query: { ...route.query, player: playerId, page: undefined },
      })
    }
  },
})
const { page, pageSize, maxPage, offset, pageLocation } = useRoutePagination({ maxOffset: 100_000 })
function comparisonPageLocation(nextPage: number) {
  const location = pageLocation(nextPage)
  return { ...location, query: { ...location.query, player: selectedRivalId.value } }
}
const rivalOptions = computed(() =>
  (rivalLists.value?.rivals ?? []).map((entry) => ({
    label: rivalName(entry),
    value: entry.player_id,
  })),
)
const comparisonUrl = computed(
  () => `/api/v1/rivals/${encodeURIComponent(selectedRivalId.value)}/comparison`,
)
const comparisonQuery = computed(() => ({ limit: pageSize, offset: offset.value }))
const {
  data: comparison,
  status: comparisonStatus,
  error: comparisonError,
  clear: clearComparison,
  execute: loadComparison,
} = await useFetch<IrRivalComparison>(comparisonUrl, {
  immediate: false,
  query: comparisonQuery,
  watch: false,
})
if (selectedRivalId.value) await loadComparison()

const removing = ref(false)
const rivalErrorDescription = computed(() =>
  rivalError.value ? translateApiError(rivalError.value, 'errors.rivalsLoadFailed') : '',
)
const comparisonErrorDescription = computed(() =>
  comparisonError.value
    ? translateApiError(comparisonError.value, 'errors.rivalComparisonLoadFailed')
    : '',
)

watch([selectedRivalId, page], () => {
  if (selectedRivalId.value) {
    void loadComparison()
  } else {
    clearComparison()
  }
})

async function removeSelectedRival() {
  if (!selectedRivalId.value) return
  removing.value = true
  try {
    await $fetch(`/api/v1/rivals/${encodeURIComponent(selectedRivalId.value)}`, {
      method: 'DELETE',
    })
    toast.add({
      title: t('rivals.removed'),
      color: 'success',
      icon: 'i-lucide-circle-check',
    })
    const remainingRival = rivalLists.value?.rivals.find(
      (entry) => entry.player_id !== selectedRivalId.value,
    )
    await navigateTo(
      {
        path: route.path,
        query: { ...route.query, player: remainingRival?.player_id, page: undefined },
      },
      { replace: true },
    )
    await refreshRivals()
  } catch (requestError) {
    toast.add({
      title: t('rivals.updateFailed'),
      description: translateApiError(requestError, 'errors.rivalUpdateFailed'),
      color: 'error',
      icon: 'i-lucide-circle-alert',
    })
  } finally {
    removing.value = false
  }
}

function queryString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null
}

function rivalName(entry: IrRivalEntry): string {
  return entry.profile?.display_name || entry.player_id
}

function signed(value: number): string {
  return value > 0 ? `+${value}` : String(value)
}

function outcomeClass(outcome: IrRivalComparisonOutcome): string {
  if (outcome === 'win') return 'text-success'
  if (outcome === 'loss') return 'text-error'
  return 'text-muted'
}

useSeoMeta({ title: () => t('rivals.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 sm:py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('rivals.title') }}</h1>
        <p class="mt-2 text-sm text-muted">{{ t('rivals.description') }}</p>
      </div>

      <UAlert
        v-if="rivalError"
        color="error"
        icon="i-lucide-circle-alert"
        :description="rivalErrorDescription"
      />
      <div v-else-if="rivalStatus === 'pending'" class="py-12 text-center text-muted">
        <UIcon name="i-lucide-loader-circle" class="mx-auto mb-3 size-8 animate-spin" />
        {{ t('common.loading') }}
      </div>

      <template v-else-if="rivalLists">
        <section class="mb-8 rounded-xl border border-muted bg-elevated p-5">
          <div class="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
            <div class="min-w-0 flex-1">
              <h2 class="text-xl font-semibold">{{ t('rivals.registered') }}</h2>
              <p class="mt-1 text-sm text-muted">
                {{ t('rivals.registeredCount', { count: rivalLists.rivals.length }) }}
              </p>
              <USelect
                v-if="rivalLists.rivals.length"
                v-model="selectedRivalId"
                class="mt-4 w-full sm:max-w-sm"
                :items="rivalOptions"
                icon="i-lucide-users-round"
              />
            </div>
            <UButton
              v-if="selectedRivalId"
              color="error"
              icon="i-lucide-user-minus"
              :loading="removing"
              variant="subtle"
              @click="removeSelectedRival"
            >
              {{ t('rivals.remove') }}
            </UButton>
          </div>

          <div
            v-if="!rivalLists.rivals.length"
            class="mt-5 rounded-lg border border-dashed border-muted py-10 text-center"
          >
            <UIcon name="i-lucide-user-round-search" class="mx-auto mb-3 size-8 text-muted" />
            <p class="font-medium">{{ t('rivals.empty') }}</p>
            <UButton
              class="mt-4"
              color="neutral"
              icon="i-lucide-search"
              :to="localePath('/players')"
              variant="subtle"
            >
              {{ t('rivals.findPlayers') }}
            </UButton>
          </div>
        </section>

        <section v-if="selectedRivalId" class="mb-10">
          <UAlert
            v-if="comparisonError"
            color="error"
            icon="i-lucide-circle-alert"
            :description="comparisonErrorDescription"
          />
          <div v-else-if="comparisonStatus === 'pending'" class="py-12 text-center text-muted">
            <UIcon name="i-lucide-loader-circle" class="mx-auto mb-3 size-8 animate-spin" />
            {{ t('rivals.comparisonLoading') }}
          </div>

          <template v-else-if="comparison">
            <div class="mb-5 text-center">
              <h2 class="text-2xl font-semibold">{{ t('rivals.comparison') }}</h2>
              <p class="mt-1 text-muted">
                {{ comparison.players.self.display_name }}
                <span class="mx-2 text-dimmed">vs</span>
                {{ comparison.players.rival.display_name }}
              </p>
            </div>

            <div class="mb-6 grid gap-3 sm:grid-cols-2">
              <div class="rounded-xl border border-muted bg-elevated p-5 text-center">
                <p class="text-sm font-medium text-muted">{{ t('rivals.exScoreRecord') }}</p>
                <p class="mt-2 text-2xl font-semibold">
                  <span class="text-success">{{ comparison.summary.ex_score.wins }}</span>
                  <span class="mx-2 text-dimmed">-</span>
                  <span class="text-error">{{ comparison.summary.ex_score.losses }}</span>
                  <span class="mx-2 text-dimmed">-</span>
                  <span class="text-muted">{{ comparison.summary.ex_score.draws }}</span>
                </p>
                <p class="mt-1 text-xs text-muted">{{ t('rivals.winLossDraw') }}</p>
              </div>
              <div class="rounded-xl border border-muted bg-elevated p-5 text-center">
                <p class="text-sm font-medium text-muted">{{ t('rivals.bpRecord') }}</p>
                <p class="mt-2 text-2xl font-semibold">
                  <span class="text-success">{{ comparison.summary.min_bp.wins }}</span>
                  <span class="mx-2 text-dimmed">-</span>
                  <span class="text-error">{{ comparison.summary.min_bp.losses }}</span>
                  <span class="mx-2 text-dimmed">-</span>
                  <span class="text-muted">{{ comparison.summary.min_bp.draws }}</span>
                </p>
                <p class="mt-1 text-xs text-muted">{{ t('rivals.winLossDraw') }}</p>
              </div>
            </div>

            <div
              v-if="comparison.comparisons.length"
              class="overflow-x-auto rounded-xl border border-muted"
            >
              <table class="w-full min-w-[980px] text-sm">
                <thead class="bg-elevated text-left text-muted">
                  <tr>
                    <th scope="col" class="w-px px-1">
                      <span class="sr-only">{{ t('score.title') }}</span>
                    </th>
                    <th class="w-28 px-4 py-3">LV</th>
                    <th class="px-4 py-3">{{ t('table.chart') }}</th>
                    <th class="w-44 px-4 py-3">{{ t('table.conditions') }}</th>
                    <th class="w-24 px-4 py-3 text-right">{{ t('rivals.myEx') }}</th>
                    <th class="w-24 px-4 py-3 text-right">{{ t('rivals.rivalEx') }}</th>
                    <th class="w-20 px-4 py-3 text-right">{{ t('rivals.difference') }}</th>
                    <th class="w-20 px-4 py-3 text-right">{{ t('rivals.myBp') }}</th>
                    <th class="w-20 px-4 py-3 text-right">{{ t('rivals.rivalBp') }}</th>
                    <th class="w-20 px-4 py-3 text-right">{{ t('rivals.difference') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <template
                    v-for="row in comparison.comparisons"
                    :key="`${row.chart.sha256}-${row.rule.ln_policy}-${row.rule.double_option}-${row.rule.rule_mode}-${row.rule.scoring}`"
                  >
                    <tr class="border-t border-muted">
                      <td class="px-1 py-1 align-top">
                        <ScoreDetailToggle
                          :expanded="expandedScoreRow === row.self.score_id"
                          :controls="scoreDetailsId"
                          @toggle="toggleScoreDetails(row.self.score_id)"
                        />
                      </td>
                      <td class="px-4 py-3 align-top">
                        <DifficultyTableBadges
                          v-if="row.difficulty_labels.length"
                          :labels="row.difficulty_labels"
                        />
                        <UBadge v-else color="neutral" size="sm" variant="subtle">
                          ☆{{ row.chart.level ?? '?' }}
                        </UBadge>
                      </td>
                      <td class="max-w-80 px-4 py-3 align-top">
                        <NuxtLink
                          :to="localePath(`/charts/${row.chart.sha256}`)"
                          class="font-semibold text-highlighted hover:underline"
                        >
                          {{ row.chart.title || row.chart.sha256.slice(0, 12) }}
                        </NuxtLink>
                        <p v-if="row.chart.subtitle" class="mt-0.5 text-xs text-muted">
                          {{ row.chart.subtitle }}
                        </p>
                        <p class="mt-0.5 text-xs text-muted">{{ row.chart.artist ?? '' }}</p>
                      </td>
                      <td class="px-4 py-3 align-top text-muted">
                        {{ row.rule.ln_policy }} / {{ row.rule.double_option }} /
                        {{ row.rule.rule_mode }} / {{ row.rule.scoring }}
                      </td>
                      <td class="px-4 py-3 text-right font-medium">
                        {{ row.self.ex_score }}
                      </td>
                      <td class="px-4 py-3 text-right font-medium">
                        {{ row.rival.ex_score }}
                      </td>
                      <td
                        class="px-4 py-3 text-right font-semibold"
                        :class="outcomeClass(row.outcome.ex_score)"
                      >
                        {{ signed(row.difference.ex_score) }}
                      </td>
                      <td class="px-4 py-3 text-right">{{ row.self.min_bp }}</td>
                      <td class="px-4 py-3 text-right">{{ row.rival.min_bp }}</td>
                      <td
                        class="px-4 py-3 text-right font-semibold"
                        :class="outcomeClass(row.outcome.min_bp)"
                      >
                        {{ signed(row.difference.min_bp) }}
                      </td>
                    </tr>
                    <tr
                      v-if="expandedScoreRow === row.self.score_id"
                      class="border-t border-default bg-elevated"
                    >
                      <td colspan="10">
                        <ScoreInlineDetails
                          :id="scoreDetailsId"
                          :score-id="row.self.score_id"
                          :other-score-id="row.rival.score_id"
                          :score-label="t('rivals.myEx')"
                          :other-score-label="t('rivals.rivalEx')"
                        />
                      </td>
                    </tr>
                  </template>
                </tbody>
              </table>
            </div>
            <div v-else class="rounded-lg border border-dashed border-muted py-10 text-center">
              <UIcon name="i-lucide-git-compare-arrows" class="mx-auto mb-3 size-8 text-muted" />
              <p class="font-medium">{{ t('rivals.noCommonScores') }}</p>
            </div>

            <div
              v-if="comparison.pagination.total > pageSize || page > 1"
              class="mt-5 flex justify-end"
            >
              <UPagination
                :page="
                  Math.min(page, Math.max(1, Math.ceil(comparison.pagination.total / pageSize)))
                "
                :to="comparisonPageLocation"
                :sibling-count="1"
                :items-per-page="pageSize"
                :total="Math.min(maxPage * pageSize, Math.max(1, comparison.pagination.total))"
              />
            </div>
          </template>
        </section>

        <section>
          <h2 class="text-xl font-semibold">{{ t('rivals.reverseTitle') }}</h2>
          <p class="mt-1 text-sm text-muted">
            {{ t('rivals.reverseDescription', { count: rivalLists.reverse_rivals.length }) }}
          </p>
          <p v-if="!rivalLists.reverse_rivals.length" class="mt-4 text-sm text-muted">
            {{ t('rivals.reverseEmpty') }}
          </p>
          <ul v-else class="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <li v-for="entry in rivalLists.reverse_rivals" :key="entry.player_id">
              <NuxtLink
                :to="localePath(`/players/${entry.player_id}`)"
                class="block rounded-lg border border-muted bg-elevated p-4 hover:bg-accented"
              >
                <p class="font-medium text-highlighted">{{ rivalName(entry) }}</p>
                <p v-if="entry.profile?.bio" class="mt-1 truncate text-xs text-muted">
                  {{ entry.profile.bio }}
                </p>
              </NuxtLink>
            </li>
          </ul>
        </section>
      </template>
    </section>
  </main>
</template>
