<script setup lang="ts">
import type { ScoreDetailResponse } from '../types/scoreDetails'
import { selectedScoreAttempt } from '../utils/rankingDisplay'

const props = defineProps<{
  scoreId: string
  otherScoreId?: string | null
  scoreLabel?: string
  otherScoreLabel?: string
}>()
const { t } = useI18n()
const { translateApiError } = useApiError()
const selectedScore = ref(props.scoreId)
watch([() => props.scoreId, () => props.otherScoreId], ([scoreId]) => {
  selectedScore.value = scoreId
})
const scoreChoices = computed(() =>
  props.otherScoreId && props.otherScoreId !== props.scoreId
    ? [
        { value: props.scoreId, label: props.scoreLabel ?? t('score.bestEx') },
        { value: props.otherScoreId, label: props.otherScoreLabel ?? t('score.bestClear') },
      ]
    : [],
)
const { data, status, error, refresh } = useFetch<ScoreDetailResponse>(
  () => `/api/v1/scores/${encodeURIComponent(selectedScore.value)}`,
  { lazy: true },
)
const detail = computed(() => selectedScoreAttempt(selectedScore.value, data.value))
</script>

<template>
  <div class="sticky left-0 w-full max-w-[calc(100vw-3rem)] space-y-3 px-3 py-4 sm:px-4">
    <UTabs
      v-if="scoreChoices.length"
      v-model="selectedScore"
      :items="scoreChoices"
      :content="false"
      :aria-label="t('score.title')"
      size="sm"
      class="w-fit max-w-full"
    />
    <UAlert
      v-if="error"
      role="alert"
      color="error"
      :description="translateApiError(error, 'errors.scoreLoadFailed')"
    >
      <template #actions>
        <UButton color="neutral" variant="subtle" size="sm" @click="refresh()">{{
          t('score.retry')
        }}</UButton>
      </template>
    </UAlert>
    <template v-else-if="detail && status === 'success'">
      <div class="flex flex-wrap items-center gap-4">
        <ScoreRowMetadata v-bind="detail.metadata" />
        <UBadge :color="detail.verification.color" size="sm" variant="subtle">
          {{ t(detail.verification.label) }}
        </UBadge>
      </div>
      <ScoreAttemptDetails
        :key="detail.score.id"
        :score-id="detail.score.id"
        :judges="detail.score.judges"
        :replay="detail.replay"
        :client="detail.score"
      />
    </template>
    <p v-else role="status" class="text-sm text-muted">{{ t('common.loading') }}</p>
  </div>
</template>
