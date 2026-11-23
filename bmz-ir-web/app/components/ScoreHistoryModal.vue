<script setup lang="ts" generic="T extends HistoryScore">
import type { HistoryScore } from '../types/scoreHistory'

defineProps<{
  history?: { scores: T[]; pagination: { total: number } } | null
  pending: boolean
  errorDescription: string
  limit: number
  scoreKey: (score: T) => string
  scoreBp: (score: T) => number
  scoreDetailId?: (score: T) => string
  showLn: boolean
  scoreOption: (score: T) => string
}>()
const open = defineModel<boolean>('open', { required: true })
const page = defineModel<number>('page', { required: true })
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { expandedScoreRow, scoreDetailsId, toggleScoreDetails } = useScoreDisclosure()
watch([open, page], () => {
  expandedScoreRow.value = null
})

function formatScoreDate(value: string | null) {
  return value ? formatDateTime(value) : '-'
}
</script>

<template>
  <UModal v-model:open="open" :title="t('ranking.selfHistory')" :ui="{ content: 'sm:max-w-4xl' }">
    <template #body>
      <UAlert v-if="errorDescription" color="error" :description="errorDescription" class="mb-4" />
      <p v-else-if="pending" class="text-sm text-muted">{{ t('common.loading') }}</p>
      <p v-else-if="!history?.scores.length" class="text-sm text-muted">
        {{ t('ranking.noHistory') }}
      </p>
      <div v-else class="overflow-x-auto rounded-lg border border-default">
        <table class="w-full text-sm">
          <thead class="bg-elevated text-left text-toned">
            <tr>
              <th class="px-3 py-2">{{ t('table.date') }}</th>
              <th class="px-3 py-2 text-right whitespace-nowrap">EX SCORE</th>
              <th class="px-3 py-2">{{ t('table.clear') }}</th>
              <th v-if="showLn" class="px-3 py-2">LN</th>
              <th class="px-3 py-2">RULE</th>
              <th class="px-3 py-2">OPTION</th>
              <th class="px-3 py-2 text-right">COMBO</th>
              <th class="px-3 py-2 text-right">BP</th>
              <th
                v-if="scoreDetailId"
                scope="col"
                class="sticky right-0 z-10 w-px bg-elevated px-1"
              >
                <span class="sr-only">{{ t('score.title') }}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            <template v-for="score in history.scores" :key="scoreKey(score)">
              <tr class="border-t border-default">
                <td class="px-3 py-2 text-muted">
                  {{ formatScoreDate(score.played_at ?? score.server_received_at) }}
                </td>
                <td class="px-3 py-2 text-right">
                  <RankingExScore :score="score.ex_score" :max="score.max_ex_score" />
                </td>
                <td class="px-3 py-2"><IrClearBadge :clear="score.clear" /></td>
                <td v-if="showLn" class="px-3 py-2 text-muted">
                  {{ lnPolicyLabel(score.ln_policy) }}
                </td>
                <td class="px-3 py-2 text-muted">{{ ruleModeLabel(score.rule_mode) }}</td>
                <td class="px-3 py-2 text-xs text-muted">{{ scoreOption(score) }}</td>
                <td class="px-3 py-2 text-right">{{ score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ scoreBp(score) }}</td>
                <td v-if="scoreDetailId" class="sticky right-0 z-10 bg-default px-1 py-1">
                  <ScoreDetailToggle
                    :expanded="expandedScoreRow === scoreKey(score)"
                    :controls="scoreDetailsId"
                    @toggle="toggleScoreDetails(scoreKey(score))"
                  />
                </td>
              </tr>
              <tr
                v-if="scoreDetailId && expandedScoreRow === scoreKey(score)"
                class="border-t border-default bg-elevated"
              >
                <td :colspan="showLn ? 9 : 8">
                  <ScoreInlineDetails :id="scoreDetailsId" :score-id="scoreDetailId(score)" />
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
      <div v-if="history && history.pagination.total > limit" class="mt-4 flex justify-end">
        <UPagination
          v-model:page="page"
          :items-per-page="limit"
          :total="history.pagination.total"
        />
      </div>
    </template>
  </UModal>
</template>
