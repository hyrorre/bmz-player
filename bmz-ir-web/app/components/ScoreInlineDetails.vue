<script setup lang="ts">
import type { IrJudges } from '../../shared/types/ir'
import type { ScoreReplay } from '../types/scoreDetails'

const props = defineProps<{
  scoreId: string
  otherScoreId?: string | null
  scoreLabel?: string
  otherScoreLabel?: string
}>()
const { t } = useI18n()
const { translateApiError } = useApiError()
const selectedScore = ref(props.scoreId)
const scoreChoices = computed(() =>
  props.otherScoreId && props.otherScoreId !== props.scoreId
    ? [
        { value: props.scoreId, label: props.scoreLabel ?? t('score.bestEx') },
        { value: props.otherScoreId, label: props.otherScoreLabel ?? t('score.bestClear') },
      ]
    : [],
)
const activeScoreId = computed(() =>
  selectedScore.value === props.otherScoreId ? selectedScore.value : props.scoreId,
)
const { data, status, error, refresh } = useFetch<{
  score: { id: string; judges: Partial<IrJudges> | null }
  replay: ScoreReplay | null
}>(() => `/api/v1/scores/${encodeURIComponent(activeScoreId.value)}`, { lazy: true })
const detail = computed(() => (data.value?.score.id === activeScoreId.value ? data.value : null))
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
    <ScoreAttemptDetails
      v-else-if="detail && status === 'success'"
      :key="detail.score.id"
      :score-id="detail.score.id"
      :judges="detail.score.judges"
      :replay="detail.replay"
    />
    <p v-else role="status" class="text-sm text-muted">{{ t('common.loading') }}</p>
  </div>
</template>
