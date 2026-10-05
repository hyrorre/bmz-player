<script setup lang="ts" generic="T extends HistoryScore">
import type { HistoryScore } from '../types/scoreHistory'

defineProps<{
  history?: { scores: T[]; pagination: { total: number } } | null
  pending: boolean
  errorDescription: string
  limit: number
  scoreKey: (score: T) => string
  scoreBp: (score: T) => number
  scoreTo?: (score: T) => string
}>()
const open = defineModel<boolean>('open', { required: true })
const page = defineModel<number>('page', { required: true })
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()

function formatScoreDate(value: string | null) {
  return value ? formatDateTime(value) : '-'
}
</script>

<template>
  <UModal v-model:open="open" :title="t('ranking.selfHistory')">
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
              <th class="px-3 py-2 text-right">EX</th>
              <th class="px-3 py-2">{{ t('table.clear') }}</th>
              <th class="px-3 py-2 text-right">COMBO</th>
              <th class="px-3 py-2 text-right">BP</th>
              <th class="px-3 py-2">{{ t('table.conditions') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="score in history.scores"
              :key="scoreKey(score)"
              class="border-t border-default"
            >
              <td class="px-3 py-2 text-muted">
                {{ formatScoreDate(score.played_at ?? score.server_received_at) }}
              </td>
              <td class="px-3 py-2 text-right font-medium">
                <NuxtLink v-if="scoreTo" :to="scoreTo(score)" class="hover:underline">{{
                  score.ex_score
                }}</NuxtLink>
                <template v-else>{{ score.ex_score }}</template>
              </td>
              <td class="px-3 py-2">{{ score.clear }}</td>
              <td class="px-3 py-2 text-right">{{ score.max_combo }}</td>
              <td class="px-3 py-2 text-right">{{ scoreBp(score) }}</td>
              <td class="px-3 py-2 text-muted">
                <p>{{ score.gauge }} / {{ score.ln_policy }} / {{ score.rule_mode }}</p>
                <slot name="conditions" :score="score" />
              </td>
            </tr>
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
